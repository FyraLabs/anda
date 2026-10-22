use crate::{error::AndaxRes, run::rf};

use git2::Remote;
use rhai::{
    plugin::{export_module, Dynamic, EvalAltResult, NativeCallContext},
    CustomType,
};
use semver::Version;
use serde_json::Value;
use std::env::VarError;
use tracing::trace;

type Res<T> = Result<T, Box<EvalAltResult>>;

fn sort_git_tags_impl(tags: &mut [String]) {
    tags.sort_by(|a, b| {
        let parse = |tag: &str| {
            tag.char_indices()
                .rev()
                .filter(|(_, character)| character.is_ascii_digit())
                .find_map(|(index, _)| Version::parse(&tag[index..]).ok())
        };
        let a_version = parse(a);
        let b_version = parse(b);

        match (a_version, b_version) {
            (Some(a), Some(b)) => a.cmp(&b).then_with(|| a.to_string().cmp(&b.to_string())),
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (None, None) => a.cmp(b),
        }
    });
}

pub const USER_AGENT: &str = "AndaX";

/// Lookups against OCI distribution registries (GHCR, Docker Hub, Quay, …).
mod oci {
    use super::USER_AGENT;
    use serde_json::Value;
    use sha2::{Digest, Sha256};
    use std::{
        cell::RefCell,
        collections::{HashMap, HashSet},
        fmt::Write,
    };
    use tracing::trace;
    use ureq::{http::Response, Body};

    /// A failed OCI lookup; the Rhai functions wrap it with `ehdl` for script tracebacks.
    #[derive(Debug)]
    pub struct Error(String);

    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0)
        }
    }

    impl std::error::Error for Error {}

    impl From<String> for Error {
        fn from(message: String) -> Self {
            Self(message)
        }
    }

    impl From<&str> for Error {
        fn from(message: &str) -> Self {
            Self(message.to_owned())
        }
    }

    pub type Res<T> = Result<T, Error>;

    /// Index/list types come first so multi-arch images resolve to the index digest
    /// rather than the digest of a single architecture's manifest.
    pub const MANIFEST_ACCEPT: &str = "application/vnd.oci.image.index.v1+json, \
        application/vnd.docker.distribution.manifest.list.v2+json, \
        application/vnd.oci.image.manifest.v1+json, \
        application/vnd.docker.distribution.manifest.v2+json";

    const MAX_TAG_PAGES: usize = 1000;

    thread_local! {
        /// Anonymous pull tokens by (API base URL, repository).
        /// `anda update` runs every update script on its own thread, so this lasts for one run.
        static TOKENS: RefCell<HashMap<(String, String), String>> = RefCell::new(HashMap::new());
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ImageRef {
        /// Registry as written in the reference, e.g. `ghcr.io` or `docker.io`.
        pub registry: String,
        /// Repository within the registry, e.g. `library/alpine`.
        pub repository: String,
        /// Base URL of the registry API, e.g. `https://registry-1.docker.io`.
        pub api_base: String,
    }

    impl std::fmt::Display for ImageRef {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}/{}", self.registry, self.repository)
        }
    }

    impl ImageRef {
        fn is_https(&self) -> bool {
            self.api_base.starts_with("https://")
        }

        /// Parses `ghcr.io/org/name`, `docker.io/library/name`, `org/name` or `name`.
        /// Bare names go to Docker Hub, like `docker pull` does.
        pub fn parse(image: &str) -> Res<Self> {
            if image.is_empty() {
                return Err(Error::from("OCI image reference is empty."));
            }
            if image.contains("://") {
                return Err(Error::from(format!(
                    "OCI image reference `{image}` must not include a URL scheme; use e.g. `ghcr.io/org/name`."
                )));
            }
            if image.contains('@') {
                return Err(Error::from(format!(
                    "OCI image reference `{image}` already contains a digest; pass only the image name."
                )));
            }

            let (registry, path) = match image.split_once('/') {
                Some((first, rest)) if first.contains(['.', ':']) || first == "localhost" => {
                    (first, rest)
                }
                _ => ("docker.io", image),
            };
            if path.rsplit('/').next().is_some_and(|name| name.contains(':')) {
                return Err(Error::from(format!(
                    "OCI image reference `{image}` already contains a tag; pass only the image name."
                )));
            }
            if !path.split('/').all(valid_path_component) {
                return Err(Error::from(format!(
                    "OCI image reference `{image}` has an invalid repository name `{path}`."
                )));
            }

            let (registry, host) = match registry {
                "docker.io" | "index.docker.io" | "registry-1.docker.io" => {
                    ("docker.io", "registry-1.docker.io")
                }
                other => (other, other),
            };
            let repository = if registry == "docker.io" && !path.contains('/') {
                format!("library/{path}")
            } else {
                path.to_owned()
            };

            // Like Docker, talk plain HTTP to registries on this machine (e.g. a local `registry:2`).
            let scheme = if is_loopback(host) { "http" } else { "https" };

            Ok(Self {
                registry: registry.to_owned(),
                repository,
                api_base: format!("{scheme}://{host}"),
            })
        }
    }

    /// Whether `host[:port]` names this machine: `localhost` or a loopback IP address.
    fn is_loopback(authority: &str) -> bool {
        let host = match authority.rsplit_once(':') {
            Some((host, port)) if !host.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
                host
            }
            _ => authority,
        };
        if let Some(ip) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            return ip.parse::<std::net::Ipv6Addr>().is_ok_and(|ip| ip.is_loopback());
        }
        host.eq_ignore_ascii_case("localhost")
            || host.parse::<std::net::Ipv4Addr>().is_ok_and(|ip| ip.is_loopback())
    }

    /// Checks one repository path component against the OCI distribution grammar:
    /// `[a-z0-9]+((\.|_|__|-+)[a-z0-9]+)*`.
    fn valid_path_component(component: &str) -> bool {
        let alnum = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
        let bytes = component.as_bytes();
        if !bytes.first().is_some_and(|&b| alnum(b)) || !bytes.last().is_some_and(|&b| alnum(b)) {
            return false;
        }
        bytes
            .split(|&b| alnum(b))
            .filter(|sep| !sep.is_empty())
            .all(|sep| matches!(sep, b"." | b"_" | b"__") || sep.iter().all(|&b| b == b'-'))
    }

    #[derive(Debug, PartialEq, Eq)]
    pub struct BearerChallenge {
        pub realm: String,
        pub service: Option<String>,
    }

    /// Parses the Bearer challenge in a `WWW-Authenticate` header value such as
    /// `Bearer realm="…",service="…",scope="…"`, which may also list other schemes.
    pub fn parse_bearer_challenge(header: &str) -> Option<BearerChallenge> {
        let params = bearer_params(header)?;
        let mut params = parse_auth_params(params);
        Some(BearerChallenge { realm: params.remove("realm")?, service: params.remove("service") })
    }

    /// Returns what follows the `Bearer` scheme token, skipping quoted strings and auth params.
    fn bearer_params(header: &str) -> Option<&str> {
        let mut in_quotes = false;
        let mut escaped = false;
        // A scheme token can only start at the beginning or after a comma.
        let mut token_start = true;
        for (index, c) in header.char_indices() {
            if in_quotes {
                match (escaped, c) {
                    (true, _) => escaped = false,
                    (false, '\\') => escaped = true,
                    (false, '"') => in_quotes = false,
                    _ => {}
                }
                continue;
            }
            match c {
                '"' => in_quotes = true,
                ',' => token_start = true,
                c if c.is_whitespace() => {}
                _ if token_start => {
                    token_start = false;
                    let rest = &header[index..];
                    let end = rest.find(|c: char| c.is_whitespace() || matches!(c, '=' | ','));
                    let (scheme, after) = rest.split_at(end.unwrap_or(rest.len()));
                    let is_scheme = !after.trim_start().starts_with('=');
                    if is_scheme && scheme.eq_ignore_ascii_case("bearer") {
                        return Some(after);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Parses `key=value, key="quoted, value"` pairs, stopping at the next challenge's scheme.
    fn parse_auth_params(params: &str) -> HashMap<String, String> {
        let mut out = HashMap::new();
        let mut chars = params.chars().peekable();
        loop {
            while chars.next_if(|c| c.is_whitespace() || *c == ',').is_some() {}
            let key: String = std::iter::from_fn(|| {
                chars.next_if(|c| !matches!(c, '=' | ',') && !c.is_whitespace())
            })
            .collect();
            while chars.next_if(|c| c.is_whitespace()).is_some() {}
            if key.is_empty() || chars.next_if_eq(&'=').is_none() {
                break;
            }
            while chars.next_if(|c| c.is_whitespace()).is_some() {}
            let value = if chars.next_if_eq(&'"').is_some() {
                parse_quoted(&mut chars)
            } else {
                std::iter::from_fn(|| chars.next_if(|c| *c != ',' && !c.is_whitespace())).collect()
            };
            out.insert(key.to_ascii_lowercase(), value);
        }
        out
    }

    /// Reads the rest of a quoted string whose opening quote was already consumed.
    fn parse_quoted<I: Iterator<Item = char>>(chars: &mut I) -> String {
        let mut value = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => value.extend(chars.next()),
                c => value.push(c),
            }
        }
        value
    }

    /// Returns the target of the `rel="next"` entry in a `Link` header value.
    pub fn parse_next_link(header: &str) -> Option<String> {
        let mut rest = header;
        while let Some((_, link)) = rest.split_once('<') {
            let (target, params) = link.split_once('>')?;
            let (params, next) = params.split_at(params.find('<').unwrap_or(params.len()));
            let is_next = params.split(';').skip(1).any(|param| {
                param.split_once('=').is_some_and(|(key, value)| {
                    key.trim().eq_ignore_ascii_case("rel")
                        && value
                            .trim()
                            .trim_end_matches(',')
                            .trim()
                            .trim_matches('"')
                            .split_whitespace()
                            .any(|rel| rel.eq_ignore_ascii_case("next"))
                })
            });
            if is_next {
                return Some(target.to_owned());
            }
            rest = next;
        }
        None
    }

    /// Parses a release version tag: two or more dot-separated numbers with an optional
    /// leading `v`, e.g. `1.2.3`, `v0.10.0`, `17.6` or CalVer like `2024.01.05`.
    ///
    /// Unlike `sort_git_tags`, any suffix disqualifies the tag: image registries carry variant
    /// tags such as `10.0.0-alpine` and prereleases next to the plain releases, and those must
    /// never win over the release itself. Bare numbers are skipped too, since they are often
    /// build numbers or dates (`20240105`) that would outrank every real version.
    fn release_version(tag: &str) -> Option<Vec<u64>> {
        let parts: Vec<u64> = (tag.strip_prefix('v').unwrap_or(tag).split('.'))
            .map(|part| {
                let numeric = !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
                numeric.then(|| part.parse().ok()).flatten()
            })
            .collect::<Option<_>>()?;
        (parts.len() >= 2).then_some(parts)
    }

    /// Picks the tag with the highest release version (see `release_version`).
    pub fn newest_version_tag(tags: &[String]) -> Option<&str> {
        let mut latest: Option<(Vec<u64>, &str)> = None;
        for tag in tags {
            let Some(version) = release_version(tag) else { continue };
            if latest
                .as_ref()
                .is_none_or(|current| (&version, tag.as_str()) > (&current.0, current.1))
            {
                latest = Some((version, tag));
            }
        }
        latest.map(|(_, tag)| tag)
    }

    fn is_sha256_digest(digest: &str) -> bool {
        digest.strip_prefix("sha256:").is_some_and(|hex| {
            hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    }

    fn sha256_digest(bytes: &[u8]) -> String {
        Sha256::digest(bytes).iter().fold(String::from("sha256:"), |mut out, byte| {
            write!(out, "{byte:02x}").expect("writing to a String cannot fail");
            out
        })
    }

    #[derive(Clone, Copy)]
    enum Method {
        Get,
        Head,
    }

    fn send_once(
        agent: &ureq::Agent,
        method: Method,
        url: &str,
        accept: &str,
        token: Option<&str>,
        image: &ImageRef,
    ) -> Res<Response<Body>> {
        let req = match method {
            Method::Get => agent.get(url),
            Method::Head => agent.head(url),
        };
        let mut req = req.header("User-Agent", USER_AGENT).header("Accept", accept);
        if let Some(token) = token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        (req.config().http_status_as_error(false).https_only(image.is_https()).build().call())
            .map_err(|err| Error::from(format!("Request to `{url}` failed: {err}")))
    }

    /// Sends a request, answering a Bearer challenge with an anonymous pull token if needed.
    fn send(
        agent: &ureq::Agent,
        method: Method,
        url: &str,
        accept: &str,
        image: &ImageRef,
    ) -> Res<Response<Body>> {
        // Tokens are cached per origin so one host's token is never handed to another
        // (e.g. an absolute pagination link); each host's own challenge gets its own token.
        let key = (origin(url).to_owned(), image.repository.clone());
        let cached = TOKENS.with_borrow(|t| t.get(&key).cloned());

        let resp = send_once(agent, method, url, accept, cached.as_deref(), image)?;
        if resp.status() != 401 {
            return Ok(resp);
        }
        let challenge = (resp.headers().get_all("WWW-Authenticate").iter())
            .filter_map(|value| value.to_str().ok())
            .find_map(parse_bearer_challenge);
        let Some(challenge) = challenge else { return Ok(resp) };

        let token = fetch_token(agent, &challenge, image)?;
        TOKENS.with_borrow_mut(|t| t.insert(key, token.clone()));
        send_once(agent, method, url, accept, Some(&token), image)
    }

    /// `scheme://host[:port]` of an absolute URL.
    fn origin(url: &str) -> &str {
        let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
        let path = rest.find(['/', '?', '#']).map_or("", |i| &rest[i..]);
        &url[..url.len().saturating_sub(path.len())]
    }

    /// Rejects plain-HTTP URLs a registry hands out, except loopback URLs from a loopback registry.
    fn check_no_downgrade(url: &str, image: &ImageRef) -> Res<()> {
        let local = (origin(url).strip_prefix("http://"))
            .is_some_and(|authority| !image.is_https() && is_loopback(authority));
        if !url.starts_with("https://") && !local {
            return Err(Error::from(format!(
                "OCI registry for `{image}` sent non-HTTPS URL `{url}`; refusing to use it."
            )));
        }
        Ok(())
    }

    fn fetch_token(
        agent: &ureq::Agent,
        challenge: &BearerChallenge,
        image: &ImageRef,
    ) -> Res<String> {
        let realm = &challenge.realm;
        check_no_downgrade(realm, image)?;
        let mut req = agent.get(realm).header("User-Agent", USER_AGENT);
        if let Some(service) = &challenge.service {
            req = req.query("service", service);
        }
        let req = req.query("scope", format!("repository:{}:pull", image.repository));
        let resp =
            (req.config().http_status_as_error(false).https_only(image.is_https()).build().call())
                .map_err(|err| Error::from(format!("Request to `{realm}` failed: {err}")))?;
        match resp.status().as_u16() {
            200 => {}
            401 | 403 | 404 => {
                return Err(Error::from(format!(
                    "Could not get an anonymous pull token for OCI image `{image}` (HTTP {}); it may not exist or may be private.",
                    resp.status().as_u16()
                )))
            }
            status => return unexpected_status(status, realm, image),
        }
        let v: Value = (resp.into_body().read_json())
            .map_err(|err| Error::from(format!("Invalid token response from `{realm}`: {err}")))?;
        (v["token"].as_str().or_else(|| v["access_token"].as_str()))
            .filter(|token| !token.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| Error::from(format!("Token response from `{realm}` contains no token.")))
    }

    fn unexpected_status<T>(status: u16, url: &str, image: &ImageRef) -> Res<T> {
        Err(Error::from(format!(
            "Unexpected HTTP status {status} from `{url}` for OCI image `{image}`."
        )))
    }

    fn image_not_found<T>(status: u16, image: &ImageRef) -> Res<T> {
        Err(Error::from(format!(
            "OCI image `{image}` was not found or is not public (HTTP {status})."
        )))
    }

    /// Lists all tags, following `Link: <…>; rel="next"` pagination.
    pub fn tags(agent: &ureq::Agent, image: &ImageRef) -> Res<Vec<String>> {
        let mut url = format!("{}/v2/{}/tags/list", image.api_base, image.repository);
        let mut seen = HashSet::from([url.clone()]);
        let mut tags = vec![];
        for _ in 0..MAX_TAG_PAGES {
            let resp = send(agent, Method::Get, &url, "application/json", image)?;
            match resp.status().as_u16() {
                200 => {}
                status @ (401 | 403 | 404) => return image_not_found(status, image),
                status => return unexpected_status(status, &url, image),
            }
            let next = (resp.headers().get_all("Link").iter())
                .filter_map(|value| value.to_str().ok())
                .find_map(parse_next_link);
            let v: Value = (resp.into_body().read_json())
                .map_err(|err| Error::from(format!("Invalid tag list from `{url}`: {err}")))?;
            trace!("Got json from {url}:\n{v}");
            if !v.is_object() {
                return Err(Error::from(format!("Tag list from `{url}` is not a JSON object.")));
            }
            match &v["tags"] {
                Value::Array(page) => {
                    tags.extend(page.iter().filter_map(Value::as_str).map(ToOwned::to_owned));
                }
                Value::Null => {}
                _ => return Err(Error::from(format!("Tag list from `{url}` is not an array."))),
            }

            let Some(next) = next else { return Ok(tags) };
            url = if next.starts_with("https://") || next.starts_with("http://") {
                check_no_downgrade(&next, image)?;
                next
            } else if next.starts_with('/') {
                format!("{}{next}", image.api_base)
            } else {
                return Err(Error::from(format!(
                    "Unsupported pagination link `{next}` from `{url}`."
                )));
            };
            if !seen.insert(url.clone()) {
                return Err(Error::from(format!(
                    "Tag list pagination for `{image}` loops at `{url}`."
                )));
            }
        }
        Err(Error::from(format!("Tag list for `{image}` has more than {MAX_TAG_PAGES} pages.")))
    }

    pub fn newest_tag(agent: &ureq::Agent, image: &ImageRef) -> Res<String> {
        let tags = tags(agent, image)?;
        newest_version_tag(&tags).map(ToOwned::to_owned).ok_or_else(|| {
            Error::from(format!("No version tags could be found for OCI image `{image}`."))
        })
    }

    /// Returns the digest of the manifest (or, for multi-arch images, the index) behind `tag`.
    pub fn digest(agent: &ureq::Agent, image: &ImageRef, tag: &str) -> Res<String> {
        let valid_tag = tag.len() <= 128
            && tag.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
            && tag.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'));
        if !valid_tag {
            return Err(Error::from(format!(
                "`{tag}` is not a valid tag for OCI image `{image}`."
            )));
        }

        let url = format!("{}/v2/{}/manifests/{tag}", image.api_base, image.repository);
        let check = |resp: &Response<Body>| match resp.status().as_u16() {
            200 => Ok(()),
            404 => Err(Error::from(format!(
                "Tag `{tag}` was not found for OCI image `{image}` (HTTP 404)."
            ))),
            status @ (401 | 403) => image_not_found(status, image),
            status => unexpected_status(status, &url, image),
        };

        let resp = send(agent, Method::Head, &url, MANIFEST_ACCEPT, image)?;
        match resp.status().as_u16() {
            200 => {
                let header = (resp.headers().get("Docker-Content-Digest"))
                    .and_then(|value| value.to_str().ok())
                    .map(str::trim);
                if let Some(digest) = header.filter(|digest| is_sha256_digest(digest)) {
                    return Ok(digest.to_owned());
                }
                trace!("No sha256 Docker-Content-Digest from HEAD {url}; hashing the manifest");
            }
            // Some registries and proxies reject HEAD on manifests; GET still works there.
            status @ (400 | 405 | 501) => {
                trace!("HEAD {url} returned HTTP {status}; hashing the manifest");
            }
            // Report rate limits and outages as-is: a GET would also count as a pull.
            _ => check(&resp)?,
        }

        let resp = send(agent, Method::Get, &url, MANIFEST_ACCEPT, image)?;
        check(&resp)?;
        let body = (resp.into_body().read_to_vec())
            .map_err(|err| Error::from(format!("Could not read manifest from `{url}`: {err}")))?;
        Ok(sha256_digest(&body))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::mpsc;

        type MockResponse = tiny_http::Response<std::io::Cursor<Vec<u8>>>;

        const INDEX: &str = r#"{"schemaVersion":2,"mediaType":"application/vnd.oci.image.index.v1+json","manifests":[]}"#;

        struct Recorded {
            method: String,
            url: String,
            headers: Vec<(String, String)>,
        }

        impl From<&tiny_http::Request> for Recorded {
            fn from(request: &tiny_http::Request) -> Self {
                Self {
                    method: request.method().as_str().to_owned(),
                    url: request.url().to_owned(),
                    headers: (request.headers().iter())
                        .map(|h| (h.field.as_str().to_string(), h.value.to_string()))
                        .collect(),
                }
            }
        }

        impl Recorded {
            fn header(&self, name: &str) -> Option<&str> {
                (self.headers.iter())
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .map(|(_, value)| value.as_str())
            }
        }

        /// A local registry whose requests are answered by `handler(request, base_url)`.
        struct MockRegistry {
            image: ImageRef,
            requests: mpsc::Receiver<Recorded>,
        }

        impl MockRegistry {
            fn start<F>(handler: F) -> Self
            where
                F: Fn(&Recorded, &str) -> MockResponse + Send + 'static,
            {
                let server = tiny_http::Server::http("127.0.0.1:0").expect("mock registry binds");
                let port = server.server_addr().to_ip().expect("mock registry has an IP").port();
                let base = format!("http://127.0.0.1:{port}");
                let (tx, rx) = mpsc::channel();
                let handler_base = base.clone();
                std::thread::spawn(move || serve(&server, &handler, &tx, &handler_base));
                let image = ImageRef {
                    registry: format!("127.0.0.1:{port}"),
                    repository: "org/app".to_owned(),
                    api_base: base,
                };
                Self { image, requests: rx }
            }

            fn requests(&self) -> Vec<Recorded> {
                self.requests.try_iter().collect()
            }
        }

        /// Records each request (before answering it, so tests see it once the call returns).
        fn serve<F>(
            server: &tiny_http::Server,
            handler: &F,
            tx: &mpsc::Sender<Recorded>,
            base: &str,
        ) where
            F: Fn(&Recorded, &str) -> MockResponse,
        {
            for request in server.incoming_requests() {
                let recorded = Recorded::from(&request);
                let response = handler(&recorded, base);
                if tx.send(recorded).is_err() {
                    break;
                }
                if let Err(err) = request.respond(response) {
                    eprintln!("mock registry: {err}");
                }
            }
        }

        fn respond(status: u16, body: &str, headers: &[(&str, &str)]) -> MockResponse {
            let resp = MockResponse::from_data(body.as_bytes().to_vec()).with_status_code(status);
            headers.iter().fold(resp, |resp, (key, value)| {
                resp.with_header(
                    tiny_http::Header::from_bytes(key.as_bytes(), value.as_bytes())
                        .expect("header"),
                )
            })
        }

        fn challenge(base: &str) -> String {
            format!(
                r#"Bearer realm="{base}/token",service="mock-registry",scope="repository:org/app:pull""#
            )
        }

        fn agent() -> ureq::Agent {
            ureq::Agent::new_with_config(
                ureq::Agent::config_builder()
                    .timeout_global(Some(std::time::Duration::from_secs(10)))
                    .proxy(None)
                    .build(),
            )
        }

        fn percent_decode(s: &str) -> String {
            let mut out = vec![];
            let mut bytes = s.bytes();
            while let Some(b) = bytes.next() {
                if b != b'%' {
                    out.push(b);
                    continue;
                }
                let hex: String = bytes.by_ref().take(2).map(char::from).collect();
                out.push(u8::from_str_radix(&hex, 16).expect("percent escape"));
            }
            String::from_utf8(out).expect("utf-8")
        }

        fn error_message<T: std::fmt::Debug>(result: Res<T>) -> String {
            result.expect_err("expected an error").to_string()
        }

        #[test]
        fn parses_image_references() {
            let cases = [
                (
                    "ghcr.io/ultramarine-linux/dashboard",
                    "ghcr.io",
                    "ultramarine-linux/dashboard",
                    "https://ghcr.io",
                ),
                ("quay.io/org/name", "quay.io", "org/name", "https://quay.io"),
                (
                    "docker.io/library/alpine",
                    "docker.io",
                    "library/alpine",
                    "https://registry-1.docker.io",
                ),
                ("docker.io/alpine", "docker.io", "library/alpine", "https://registry-1.docker.io"),
                (
                    "index.docker.io/org/name",
                    "docker.io",
                    "org/name",
                    "https://registry-1.docker.io",
                ),
                ("alpine", "docker.io", "library/alpine", "https://registry-1.docker.io"),
                ("org/name", "docker.io", "org/name", "https://registry-1.docker.io"),
                ("ghcr.io/org/group/name", "ghcr.io", "org/group/name", "https://ghcr.io"),
                ("localhost:5000/name", "localhost:5000", "name", "http://localhost:5000"),
                ("localhost/name", "localhost", "name", "http://localhost"),
                ("127.0.0.1:5000/org/name", "127.0.0.1:5000", "org/name", "http://127.0.0.1:5000"),
                ("[::1]:5000/name", "[::1]:5000", "name", "http://[::1]:5000"),
                (
                    "[0:0:0:0:0:0:0:1]:5000/n",
                    "[0:0:0:0:0:0:0:1]:5000",
                    "n",
                    "http://[0:0:0:0:0:0:0:1]:5000",
                ),
                ("[2001:db8::1]:5000/n", "[2001:db8::1]:5000", "n", "https://[2001:db8::1]:5000"),
                (
                    "registry.local:5000/name",
                    "registry.local:5000",
                    "name",
                    "https://registry.local:5000",
                ),
                ("ghcr.io/a__b/c-d/e--f/g.h", "ghcr.io", "a__b/c-d/e--f/g.h", "https://ghcr.io"),
            ];
            for (input, registry, repository, api_base) in cases {
                let image = ImageRef::parse(input).unwrap_or_else(|err| panic!("{input}: {err}"));
                assert_eq!(
                    image,
                    ImageRef {
                        registry: registry.to_owned(),
                        repository: repository.to_owned(),
                        api_base: api_base.to_owned()
                    },
                    "{input}"
                );
            }
        }

        #[test]
        fn rejects_invalid_image_references() {
            let cases = [
                ("alpine:3", "already contains a tag"),
                ("ghcr.io/org/name:v1.0.0", "already contains a tag"),
                ("localhost:5000/name:latest", "already contains a tag"),
                ("ghcr.io/org/name@sha256:0123", "already contains a digest"),
                ("ghcr.io/org/name:v1@sha256:0123", "already contains a digest"),
                ("https://ghcr.io/org/name", "must not include a URL scheme"),
                ("", "is empty"),
                ("ghcr.io/Org/Name", "invalid repository name"),
                ("ghcr.io//name", "invalid repository name"),
                ("ghcr.io/", "invalid repository name"),
                ("ghcr.io/-org/name", "invalid repository name"),
                ("ghcr.io/org/.name", "invalid repository name"),
                ("ghcr.io/org/name-", "invalid repository name"),
                ("ghcr.io/org/a..b", "invalid repository name"),
                ("ghcr.io/org/a___b", "invalid repository name"),
                ("ghcr.io/org/a._b", "invalid repository name"),
            ];
            for (input, expected) in cases {
                let message = error_message(ImageRef::parse(input));
                assert!(message.contains(expected), "{input:?}: {message}");
            }
        }

        #[test]
        fn picks_newest_version_tag() {
            let tags: Vec<String> = [
                "latest",
                "main",
                "sha-1a2b3c4",
                "1.2",
                "v1.2.3",
                "1.10.0",
                "9.1.0",
                "10.0.0",
                "11.0.0-rc.1",
                "v12.0.0-beta",
                "edge",
                "10.0.0-alpine",
            ]
            .map(str::to_owned)
            .into();
            assert_eq!(newest_version_tag(&tags), Some("10.0.0"));

            let tags: Vec<String> = ["v0.9.0", "v0.10.0", "v0.10.0-rc.2"].map(str::to_owned).into();
            assert_eq!(newest_version_tag(&tags), Some("v0.10.0"));

            // Two-part and zero-padded CalVer tags count; bare numbers and suffixes don't.
            let tags: Vec<String> = ["16", "17", "16.4", "17.0", "17.6", "17.6-alpine", "20240105"]
                .map(str::to_owned)
                .into();
            assert_eq!(newest_version_tag(&tags), Some("17.6"));
            let tags: Vec<String> =
                ["2023.12.31", "2024.01.05", "2024.1.4", "2024.02.0-rc1"].map(str::to_owned).into();
            assert_eq!(newest_version_tag(&tags), Some("2024.01.05"));
            let tags: Vec<String> = ["1.2", "1.2.0"].map(str::to_owned).into();
            assert_eq!(newest_version_tag(&tags), Some("1.2.0"));

            let tags: Vec<String> =
                ["latest", "main", "1", "2.0.0-rc.1", "1..2", "v", "1.2.", "1.2.3+build"]
                    .map(str::to_owned)
                    .into();
            assert_eq!(newest_version_tag(&tags), None);
            assert_eq!(newest_version_tag(&[]), None);
        }

        #[test]
        fn picks_same_tag_for_equal_versions_regardless_of_order() {
            let tags: Vec<String> = ["1.2.3", "v1.2.3"].map(str::to_owned).into();
            let reversed: Vec<String> = tags.iter().rev().cloned().collect();
            assert_eq!(newest_version_tag(&tags), Some("v1.2.3"));
            assert_eq!(newest_version_tag(&reversed), Some("v1.2.3"));
        }

        #[test]
        fn parses_bearer_challenges() {
            let cases = [
                (
                    r#"Bearer realm="https://ghcr.io/token",service="ghcr.io",scope="repository:org/name:pull""#,
                    "https://ghcr.io/token",
                    Some("ghcr.io"),
                ),
                (
                    r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:library/alpine:pull""#,
                    "https://auth.docker.io/token",
                    Some("registry.docker.io"),
                ),
                (
                    r#"Bearer realm="https://quay.io/v2/auth",service="quay.io",scope="repository:org/name:pull""#,
                    "https://quay.io/v2/auth",
                    Some("quay.io"),
                ),
                (r#"bearer  Realm = "r" , service=s"#, "r", Some("s")),
                (
                    r#"Bearer scope="repository:a/b:pull,push",realm="r",service="s""#,
                    "r",
                    Some("s"),
                ),
                (r#"Bearer realm="a\"b""#, r#"a"b"#, None),
                (r#"Bearer realm="r", Basic service="other""#, "r", None),
                (
                    r#"Basic realm="x", Bearer realm="https://r/token",service="r""#,
                    "https://r/token",
                    Some("r"),
                ),
                (r#"Basic realm="a, Bearer realm=fake", Bearer realm="real""#, "real", None),
            ];
            for (header, realm, service) in cases {
                assert_eq!(
                    parse_bearer_challenge(header),
                    Some(BearerChallenge {
                        realm: realm.to_owned(),
                        service: service.map(str::to_owned)
                    }),
                    "{header}"
                );
            }

            for header in [
                r#"Basic realm="registry""#,
                r#"Bearer service="s""#,
                "Bearer",
                "",
                r#"Basic bearer="x", realm="y""#,
            ] {
                assert_eq!(parse_bearer_challenge(header), None, "{header}");
            }
        }

        #[test]
        fn parses_next_links() {
            let cases = [
                (
                    r#"</v2/org/app/tags/list?last=b&n=2>; rel="next""#,
                    Some("/v2/org/app/tags/list?last=b&n=2"),
                ),
                (
                    "<https://r.example/v2/x/tags/list?last=b>; rel=next",
                    Some("https://r.example/v2/x/tags/list?last=b"),
                ),
                (r#"</prev>; rel="prev", </next>; rel="next""#, Some("/next")),
                (r#"</a>; title="x"; REL="Next""#, Some("/a")),
                (r#"</prev>; rel="prev""#, None),
                ("", None),
            ];
            for (header, expected) in cases {
                assert_eq!(parse_next_link(header).as_deref(), expected, "{header}");
            }
        }

        #[test]
        fn computes_sha256_digests() {
            assert_eq!(
                sha256_digest(b"abc"),
                "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            );
            assert!(is_sha256_digest(&sha256_digest(b"")));
            assert!(!is_sha256_digest("sha256:ABC"));
            assert!(!is_sha256_digest("sha512:abc"));
        }

        #[test]
        fn fetches_and_caches_anonymous_token() {
            let mock = MockRegistry::start(|req, base| match req.url.as_str() {
                url if url.starts_with("/token?") => {
                    let query = percent_decode(url);
                    if query.contains("service=mock-registry")
                        && query.contains("scope=repository:org/app:pull")
                    {
                        respond(200, r#"{"token":"t0k3n"}"#, &[])
                    } else {
                        respond(400, "bad token request", &[])
                    }
                }
                "/v2/org/app/tags/list" if req.header("Authorization") == Some("Bearer t0k3n") => {
                    respond(200, r#"{"name":"org/app","tags":["1.0.0","latest"]}"#, &[])
                }
                "/v2/org/app/tags/list" => {
                    respond(401, "", &[("WWW-Authenticate", &challenge(base))])
                }
                _ => respond(404, "", &[]),
            });
            let agent = agent();

            assert_eq!(tags(&agent, &mock.image).expect("tags"), ["1.0.0", "latest"]);
            let requests = mock.requests();
            let seen: Vec<_> =
                requests.iter().map(|r| r.url.split('?').next().unwrap_or("")).collect();
            assert_eq!(seen, ["/v2/org/app/tags/list", "/token", "/v2/org/app/tags/list"]);
            assert_eq!(requests[0].header("Authorization"), None);
            assert_eq!(requests[2].header("Authorization"), Some("Bearer t0k3n"));

            // The second lookup reuses the cached token.
            assert_eq!(newest_tag(&agent, &mock.image).expect("tag"), "1.0.0");
            let requests = mock.requests();
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].header("Authorization"), Some("Bearer t0k3n"));
        }

        #[test]
        fn replaces_rejected_cached_token() {
            let mock = MockRegistry::start(|req, base| match req.url.as_str() {
                url if url.starts_with("/token?") => {
                    respond(200, r#"{"access_token":"fresh"}"#, &[])
                }
                "/v2/org/app/tags/list" if req.header("Authorization") == Some("Bearer fresh") => {
                    respond(200, r#"{"name":"org/app","tags":["v2.0.0"]}"#, &[])
                }
                _ => respond(401, "", &[("WWW-Authenticate", &challenge(base))]),
            });
            let key = (mock.image.api_base.clone(), mock.image.repository.clone());
            TOKENS.with_borrow_mut(|t| t.insert(key, "stale".to_owned()));

            assert_eq!(tags(&agent(), &mock.image).expect("tags"), ["v2.0.0"]);
            let auth: Vec<_> = mock
                .requests()
                .iter()
                .map(|r| r.header("Authorization").map(str::to_owned))
                .collect();
            assert_eq!(
                auth,
                [Some("Bearer stale".to_owned()), None, Some("Bearer fresh".to_owned())]
            );
        }

        #[test]
        fn follows_tag_pagination() {
            let mock = MockRegistry::start(|req, base| match req.url.as_str() {
                "/v2/org/app/tags/list" => respond(
                    200,
                    r#"{"name":"org/app","tags":["a","b"]}"#,
                    &[("Link", r#"</v2/org/app/tags/list?last=b&n=2>; rel="next""#)],
                ),
                "/v2/org/app/tags/list?last=b&n=2" => respond(
                    200,
                    r#"{"name":"org/app","tags":["c","d"]}"#,
                    &[(
                        "Link",
                        &format!(r#"<{base}/v2/org/app/tags/list?last=d&n=2>; rel="next""#),
                    )],
                ),
                "/v2/org/app/tags/list?last=d&n=2" => {
                    respond(200, r#"{"name":"org/app","tags":["e"]}"#, &[])
                }
                _ => respond(404, "", &[]),
            });

            assert_eq!(tags(&agent(), &mock.image).expect("tags"), ["a", "b", "c", "d", "e"]);
            assert_eq!(mock.requests().len(), 3);
        }

        #[test]
        fn gets_separate_token_for_other_pagination_host() {
            let cdn = MockRegistry::start(|req, base| match req.url.as_str() {
                url if url.starts_with("/token?") => respond(200, r#"{"token":"cdn"}"#, &[]),
                "/v2/org/app/tags/list?last=a"
                    if req.header("Authorization") == Some("Bearer cdn") =>
                {
                    respond(200, r#"{"name":"org/app","tags":["b"]}"#, &[])
                }
                _ => respond(401, "", &[("WWW-Authenticate", &challenge(base))]),
            });
            let next =
                format!(r#"<{}/v2/org/app/tags/list?last=a>; rel="next""#, cdn.image.api_base);
            let mock = MockRegistry::start(move |req, base| match req.url.as_str() {
                url if url.starts_with("/token?") => respond(200, r#"{"token":"main"}"#, &[]),
                "/v2/org/app/tags/list" if req.header("Authorization") == Some("Bearer main") => {
                    respond(200, r#"{"name":"org/app","tags":["a"]}"#, &[("Link", &next)])
                }
                _ => respond(401, "", &[("WWW-Authenticate", &challenge(base))]),
            });

            assert_eq!(tags(&agent(), &mock.image).expect("tags"), ["a", "b"]);
            // The main registry's token never reaches the other host.
            assert!(cdn
                .requests()
                .iter()
                .all(|r| r.header("Authorization") != Some("Bearer main")));
        }

        #[test]
        fn rejects_downgraded_urls() {
            let image = ImageRef::parse("ghcr.io/org/app").expect("reference");
            check_no_downgrade("https://ghcr.io/v2/org/app/tags/list", &image).expect("https");
            let message = error_message(check_no_downgrade("http://ghcr.io/v2/x", &image));
            assert!(message.contains("non-HTTPS"), "{message}");

            let local = ImageRef::parse("localhost:5000/app").expect("reference");
            check_no_downgrade("http://localhost:5000/v2/x", &local).expect("loopback");
            let message = error_message(check_no_downgrade("http://remote.example/v2/x", &local));
            assert!(message.contains("non-HTTPS"), "{message}");
        }

        #[test]
        fn hashes_manifest_when_head_is_rejected() {
            let mock =
                MockRegistry::start(|req, _| match (req.method.as_str(), req.url.as_str()) {
                    ("HEAD", _) => respond(405, "", &[]),
                    ("GET", "/v2/org/app/manifests/1.0.0") => respond(200, INDEX, &[]),
                    _ => respond(404, "", &[]),
                });
            assert_eq!(
                digest(&agent(), &mock.image, "1.0.0").expect("digest"),
                "sha256:dff9de10919148711140d349bf03f1a99eb06f94b03e51715ccebfa7cdc518e2"
            );
        }

        #[test]
        fn reports_rate_limited_head_without_get() {
            let mock = MockRegistry::start(|_, _| respond(429, "", &[]));
            let message = error_message(digest(&agent(), &mock.image, "1.0.0"));
            assert!(message.contains("Unexpected HTTP status 429"), "{message}");
            let methods: Vec<_> = mock.requests().into_iter().map(|r| r.method).collect();
            assert_eq!(methods, ["HEAD"]);
        }

        #[test]
        fn rejects_non_object_tag_list() {
            let mock = MockRegistry::start(|_, _| respond(200, "[]", &[]));
            assert!(error_message(tags(&agent(), &mock.image)).contains("not a JSON object"));
        }

        #[test]
        fn rejects_pagination_back_to_first_page() {
            let mock = MockRegistry::start(|req, _| match req.url.as_str() {
                "/v2/org/app/tags/list" => respond(
                    200,
                    r#"{"tags":["a"]}"#,
                    &[("Link", r#"</v2/org/app/tags/list?last=a>; rel="next""#)],
                ),
                _ => respond(
                    200,
                    r#"{"tags":["b"]}"#,
                    &[("Link", r#"</v2/org/app/tags/list>; rel="next""#)],
                ),
            });
            assert!(error_message(tags(&agent(), &mock.image)).contains("loops"));
            assert_eq!(mock.requests().len(), 2);
        }

        #[test]
        fn rejects_looping_pagination() {
            let mock = MockRegistry::start(|_, _| {
                respond(
                    200,
                    r#"{"name":"org/app","tags":["a"]}"#,
                    &[("Link", r#"</v2/org/app/tags/list?last=a>; rel="next""#)],
                )
            });
            assert!(error_message(tags(&agent(), &mock.image)).contains("loops"));
        }

        #[test]
        fn requests_index_digest_with_head() {
            let expected = format!("sha256:{}", "0123456789abcdef".repeat(4));
            let header = expected.clone();
            let mock =
                MockRegistry::start(move |req, _| match (req.method.as_str(), req.url.as_str()) {
                    ("HEAD", "/v2/org/app/manifests/v1.2.3") => {
                        respond(200, "", &[("Docker-Content-Digest", &header)])
                    }
                    _ => respond(404, "", &[]),
                });

            assert_eq!(digest(&agent(), &mock.image, "v1.2.3").expect("digest"), expected);
            let requests = mock.requests();
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].method, "HEAD");
            assert_eq!(
                requests[0].header("Accept"),
                Some(
                    "application/vnd.oci.image.index.v1+json, \
                     application/vnd.docker.distribution.manifest.list.v2+json, \
                     application/vnd.oci.image.manifest.v1+json, \
                     application/vnd.docker.distribution.manifest.v2+json"
                )
            );
        }

        #[test]
        fn hashes_manifest_when_digest_header_is_missing() {
            let mock = MockRegistry::start(|req, base| {
                if req.header("Authorization") != Some("Bearer t") {
                    return match req.url.as_str() {
                        url if url.starts_with("/token?") => respond(200, r#"{"token":"t"}"#, &[]),
                        _ => respond(401, "", &[("WWW-Authenticate", &challenge(base))]),
                    };
                }
                match (req.method.as_str(), req.url.as_str()) {
                    ("HEAD", "/v2/org/app/manifests/1.0.0") => respond(200, "", &[]),
                    ("GET", "/v2/org/app/manifests/1.0.0") => respond(
                        200,
                        INDEX,
                        &[("Content-Type", "application/vnd.oci.image.index.v1+json")],
                    ),
                    _ => respond(404, "", &[]),
                }
            });

            assert_eq!(
                digest(&agent(), &mock.image, "1.0.0").expect("digest"),
                "sha256:dff9de10919148711140d349bf03f1a99eb06f94b03e51715ccebfa7cdc518e2"
            );
            let requests: Vec<_> = (mock.requests().into_iter())
                .filter(|r| r.header("Authorization").is_some())
                .collect();
            let methods: Vec<_> = requests.iter().map(|r| r.method.as_str()).collect();
            assert_eq!(methods, ["HEAD", "GET"]);
            assert!(requests.iter().all(|r| r.header("Accept") == Some(MANIFEST_ACCEPT)));
        }

        #[test]
        fn reports_lookup_errors() {
            let mock = MockRegistry::start(|req, base| match req.url.as_str() {
                url if url.starts_with("/token?") => respond(403, "", &[]),
                "/v2/broken/app/tags/list" => respond(500, "", &[]),
                "/v2/private/app/tags/list" => {
                    respond(401, "", &[("WWW-Authenticate", &challenge(base))])
                }
                "/v2/org/app/tags/list" => {
                    respond(200, r#"{"name":"org/app","tags":["latest","main","sha-abc"]}"#, &[])
                }
                // Includes `missing/app` and unknown manifests.
                _ => respond(404, "", &[]),
            });
            let agent = agent();
            let image = |repository: &str| ImageRef {
                repository: repository.to_owned(),
                ..mock.image.clone()
            };

            let cases = [
                (
                    error_message(tags(&agent, &image("missing/app"))),
                    "was not found or is not public (HTTP 404)",
                ),
                (error_message(tags(&agent, &image("broken/app"))), "Unexpected HTTP status 500"),
                (
                    error_message(tags(&agent, &image("private/app"))),
                    "Could not get an anonymous pull token",
                ),
                (error_message(newest_tag(&agent, &mock.image)), "No version tags"),
                (error_message(digest(&agent, &mock.image, "9.9.9")), "Tag `9.9.9` was not found"),
                (error_message(digest(&agent, &mock.image, "sha256:abc")), "is not a valid tag"),
            ];
            for (message, expected) in cases {
                assert!(message.contains(expected), "{message}");
            }
        }

        #[test]
        fn registers_rhai_functions() {
            let (engine, _) = crate::run::gen_en();
            let cases = [
                ("let x = 1;\noci_tag(\"alpine:3\")", "oci_tag", "already contains a tag"),
                ("let x = 1;\noci_tags(\"\")", "oci_tags", "is empty"),
                (
                    "let x = 1;\noci_digest(\"a@sha256:0\", \"1\")",
                    "oci_digest",
                    "already contains a digest",
                ),
            ];
            for (script, function, expected) in cases {
                let err = engine.eval::<rhai::Dynamic>(script).expect_err(function);
                let rhai::EvalAltResult::ErrorRuntime(value, position) = *err else {
                    panic!("{function}: not a runtime error");
                };
                assert_eq!(position.line(), Some(2), "{function}");
                let Some(crate::error::AndaxError::RustError(name, _, source)) = value.try_cast()
                else {
                    panic!("{function}: not an AndaxError::RustError");
                };
                assert_eq!(name, function);
                assert!(source.to_string().contains(expected), "{function}: {source}");
            }
        }

        /// The tag's digest must be the multi-arch index, i.e. the hash of what a
        /// manifest GET returns for that tag with our Accept header.
        fn check_live_image(reference: &str) {
            let agent = agent();
            let image = ImageRef::parse(reference).expect("reference");
            let tag = newest_tag(&agent, &image).expect("tag");
            let index_digest = digest(&agent, &image, &tag).expect("digest");
            assert!(is_sha256_digest(&index_digest), "{index_digest}");

            let url = format!("{}/v2/{}/manifests/{tag}", image.api_base, image.repository);
            let resp = send(&agent, Method::Get, &url, MANIFEST_ACCEPT, &image).expect("manifest");
            assert_eq!(resp.status(), 200);
            let content_type = resp
                .headers()
                .get("Content-Type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned();
            assert!(
                content_type.starts_with("application/vnd.oci.image.index.v1+json")
                    || content_type
                        .starts_with("application/vnd.docker.distribution.manifest.list.v2+json"),
                "{reference}:{tag} is not multi-arch: {content_type}"
            );
            let body = resp.into_body().read_to_vec().expect("body");
            assert_eq!(sha256_digest(&body), index_digest);
            eprintln!("{reference}:{tag}@{index_digest}");
        }

        #[test]
        #[ignore = "needs network access to ghcr.io"]
        fn live_ghcr_multi_arch_index() {
            check_live_image("ghcr.io/astral-sh/uv");
        }

        #[test]
        #[ignore = "needs network access to docker.io"]
        fn live_docker_hub_multi_arch_index() {
            check_live_image("alpine");
        }
    }
}

#[export_module]
pub mod ar {
    type E = Box<rhai::EvalAltResult>;

    static AGENT: std::sync::LazyLock<ureq::Agent> = std::sync::LazyLock::new(|| {
        ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(10)))
                .build(),
        )
    });

    #[rhai_fn(return_raw, global)]
    pub fn sort_git_tags(tags: rhai::Array) -> Res<rhai::Array> {
        let mut tags: Vec<String> = tags
            .into_iter()
            .map(|tag| {
                tag.into_string().map_err(|_| E::from("sort_git_tags expects an array of strings"))
            })
            .collect::<Result<_, _>>()?;

        sort_git_tags_impl(&mut tags);
        Ok(tags.into_iter().map(rhai::Dynamic::from).collect())
    }

    #[rhai_fn(return_raw, global)]
    pub fn get_json(ctx: NativeCallContext, url: &str) -> Res<Dynamic> {
        let resp = AGENT.get(url).header("User-Agent", USER_AGENT).call().ehdl(&ctx)?;
        resp.into_body().read_json().ehdl(&ctx)
    }

    fn get_json_value(ctx: NativeCallContext, url: &str) -> Res<Value> {
        let resp = AGENT.get(url).header("User-Agent", USER_AGENT).call().ehdl(&ctx)?;
        resp.into_body().read_json().ehdl(&ctx)
    }

    #[rhai_fn(return_raw, global)]
    pub fn get(ctx: NativeCallContext, url: &str) -> Res<String> {
        let resp = AGENT.get(url).header("User-Agent", USER_AGENT).call().ehdl(&ctx)?;
        resp.into_body().read_to_string().ehdl(&ctx)
    }

    #[rhai_fn(return_raw, global)]
    pub fn gh(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let req = (AGENT.get(&format!("https://api.github.com/repos/{repo}/releases/latest")))
            .header("Authorization", &format!("Bearer {}", internal_env("GITHUB_TOKEN")?))
            .header("User-Agent", USER_AGENT);
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        Ok(v["tag_name"].as_str().unwrap_or("").to_owned())
    }
    #[rhai_fn(return_raw, global)]
    pub fn gh_releases(ctx: NativeCallContext, repo: &str) -> Res<rhai::Array> {
        let req = (AGENT.get(&format!("https://api.github.com/repos/{repo}/releases")))
            .header("Authorization", &format!("Bearer {}", internal_env("GITHUB_TOKEN")?))
            .header("User-Agent", USER_AGENT);
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        let releases = v.as_array().ok_or_else(|| E::from("gh_releases received not array"))?;
        let mut tags: Vec<String> = releases
            .iter()
            .filter_map(|release| release["tag_name"].as_str().map(ToOwned::to_owned))
            .collect();

        sort_git_tags(&mut tags);
        Ok(tags.into_iter().map(rhai::Dynamic::from).collect())
    }

    #[rhai_fn(return_raw, global)]
    pub fn gh_tags(ctx: NativeCallContext, repo: &str) -> Res<rhai::Array> {
        let mut all_names: Vec<String> = Vec::new();

        let mut req_url_opt: Option<String> =
            Some(format!("https://api.github.com/repos/{repo}/tags"));

        while let Some(req_url) = req_url_opt {
            let req = (AGENT.get(&req_url))
                .header("Authorization", &format!("Bearer {}", internal_env("GITHUB_TOKEN")?))
                .header("User-Agent", USER_AGENT);
            let resp = req.call().ehdl(&ctx)?;
            req_url_opt = None;

            if let Some(link_header) = resp.headers().get("link") {
                // format:
                // <URL>; param="value"; other_param="other_value", ...
                let link = link_header.to_str().unwrap_or("");
                for part in link.split(',') {
                    let Some((url, params)) = part.split_once(">;") else { continue };
                    let Some(url) = url.trim().strip_prefix('<') else { continue };

                    if params
                        .split(',')
                        .find_map(|param| param.trim().strip_prefix("rel=\""))
                        .and_then(|rel| rel.strip_suffix('"'))
                        .is_some_and(|rel| rel == "next")
                    {
                        req_url_opt = Some(url.to_owned());
                        break;
                    }
                }
            }

            let v: Value = resp.into_body().read_json().ehdl(&ctx)?;
            trace!("Got json from {repo}:\n{v}");
            let tags = v.as_array().ok_or_else(|| E::from("gh_tags received not array"))?;
            let names: Vec<String> =
                tags.iter().filter_map(|tag| tag["name"].as_str().map(ToOwned::to_owned)).collect();

            all_names.extend(names);
        }

        sort_git_tags(&mut all_names);
        Ok(all_names.into_iter().map(rhai::Dynamic::from).collect())
    }

    #[rhai_fn(return_raw, global)]
    pub fn gh_tag(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let req = (AGENT.get(&format!("https://api.github.com/repos/{repo}/tags?per_page=1")))
            .header("Authorization", &format!("Bearer {}", internal_env("GITHUB_TOKEN")?))
            .header("User-Agent", USER_AGENT);
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        let v = (v.as_array())
            .ok_or_else(|| E::from("gh_tag received not array"))
            .map(|a| a.first().ok_or_else(|| E::from("gh_tag no tags")))??;
        Ok(v["name"].as_str().unwrap_or("").to_owned())
    }
    #[rhai_fn(return_raw, global)]
    pub fn gh_commit(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let req = (AGENT.get(&format!("https://api.github.com/repos/{repo}/commits/HEAD")))
            .header("Authorization", &format!("Bearer {}", internal_env("GITHUB_TOKEN")?))
            .header("User-Agent", USER_AGENT);
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        Ok(v["sha"].as_str().unwrap_or("").to_owned())
    }
    #[rhai_fn(return_raw, global)]
    pub fn gh_rawfile(ctx: NativeCallContext, repo: &str, branch: &str, file: &str) -> Res<String> {
        get(ctx, &format!("https://raw.githubusercontent.com/{repo}/{branch}/{file}"))
    }

    #[rhai_fn(return_raw, name = "gitlab", global)]
    pub fn gitlab_domain(ctx: NativeCallContext, domain: &str, id: &str) -> Res<String> {
        let v = get_json_value(ctx, &format!("https://{domain}/api/v4/projects/{id}/releases/"))?;
        trace!("Got json from {id}:\n{v}");
        Ok(v[0]["tag_name"].as_str().unwrap_or("").to_owned())
    }
    #[rhai_fn(return_raw, global)]
    pub fn gitlab(ctx: NativeCallContext, id: &str) -> Res<String> {
        gitlab_domain(ctx, "gitlab.com", id)
    }
    #[rhai_fn(return_raw, name = "gitlab_tag", global)]
    pub fn gitlab_tag_domain(ctx: NativeCallContext, domain: &str, id: &str) -> Res<String> {
        let v =
            get_json_value(ctx, &format!("https://{domain}/api/v4/projects/{id}/repository/tags"))?;
        trace!("Got json from {id}:\n{v}");
        Ok(v[0]["name"].as_str().unwrap_or("").to_owned())
    }
    #[rhai_fn(return_raw, global)]
    pub fn gitlab_tag(ctx: NativeCallContext, id: &str) -> Res<String> {
        gitlab_tag_domain(ctx, "gitlab.com", id)
    }
    #[rhai_fn(return_raw, name = "gitlab_commit", global)]
    pub fn gitlab_commit_domain(
        ctx: NativeCallContext,
        domain: &str,
        id: &str,
        branch: &str,
    ) -> Res<String> {
        let v = get_json_value(
            ctx,
            &format!("https://{domain}/api/v4/projects/{id}/repository/branches/{branch}"),
        )?;
        trace!("Got json from {id}:\n{v}");
        Ok(v["commit"]["id"].as_str().unwrap_or("").to_owned())
    }
    #[rhai_fn(return_raw, global)]
    pub fn gitlab_commit(ctx: NativeCallContext, id: &str, branch: &str) -> Res<String> {
        gitlab_commit_domain(ctx, "gitlab.com", id, branch)
    }

    #[rhai_fn(skip)]
    fn sourcearcade_tags(ctx: NativeCallContext, repo: &str) -> Res<Vec<String>> {
        let html =
            get(ctx, &format!("https://review.sourcearcade.org/plugins/gitiles/{repo}/+refs"))?;
        Ok(html
            .split("/+/refs/tags/")
            .skip(1)
            .filter_map(|tag| tag.split('\"').next())
            .map(str::to_owned)
            .collect())
    }

    #[rhai_fn(skip)]
    fn sourcearcade_version(name: &str) -> Option<Version> {
        let version_start_index = name.find(char::is_numeric)?;
        let (_, version_str) = name.split_at(version_start_index);
        let (numeric, suffix) = version_str.split_once('-').unwrap_or((version_str, ""));
        let normalized = if numeric.matches('.').count() == 1 {
            format!("{numeric}.0{suffix}")
        } else {
            version_str.to_owned()
        };
        Version::parse(&normalized).ok()
    }

    #[rhai_fn(return_raw, global)]
    pub fn sourcearcade(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let mut latest: Option<(Version, String)> = None;
        for name in sourcearcade_tags(ctx, repo)? {
            let Some(version) = sourcearcade_version(&name) else { continue };
            if latest.as_ref().is_none_or(|(current, _)| version > *current) {
                latest = Some((version, name));
            }
        }

        latest.map(|(_, tag)| tag).ok_or_else(|| E::from("No valid version tags could be found."))
    }

    #[rhai_fn(return_raw, global)]
    pub fn sourcearcade_tag(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let mut latest: Option<(Version, String)> = None;
        for name in sourcearcade_tags(ctx, repo)? {
            let Some(version) = sourcearcade_version(&name) else { continue };
            if latest.as_ref().is_none_or(|(current, _)| version > *current) {
                latest = Some((version, name));
            }
        }

        latest.map(|(_, tag)| tag).ok_or_else(|| E::from("No valid version tags could be found."))
    }

    #[rhai_fn(return_raw, global)]
    pub fn sourcearcade_commit(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let body = get(
            ctx,
            &format!("https://review.sourcearcade.org/plugins/gitiles/{repo}/+log/refs/heads/main?n=1&format=JSON"),
        )?;
        let log: Value = serde_json::from_str(body.trim_start_matches(")]}'\n"))
            .map_err(|error| E::from(error.to_string()))?;
        log["log"][0]["commit"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| E::from("Could not find HEAD in repository log."))
    }

    #[rhai_fn(return_raw, global)]
    pub fn sourcearcade_rawfile(
        ctx: NativeCallContext,
        repo: &str,
        branch: &str,
        file: &str,
    ) -> Res<String> {
        let html = get(
            ctx,
            &format!("https://review.sourcearcade.org/plugins/gitiles/{repo}/+/refs/heads/{branch}/{file}"),
        )?;
        let Some(pre_start) = html.find("<pre") else {
            return Err(E::from("Could not find file contents in SourceArcade response."));
        };
        let Some(content_offset) = html[pre_start..].find('>') else {
            return Err(E::from("Could not find file contents in SourceArcade response."));
        };
        let Some(start) =
            pre_start.checked_add(content_offset).and_then(|start| start.checked_add(1))
        else {
            return Err(E::from("Could not find file contents in SourceArcade response."));
        };
        let Some(end) = html[start..].find("</pre>") else {
            return Err(E::from("Could not find file contents in SourceArcade response."));
        };
        let Some(end) = start.checked_add(end) else {
            return Err(E::from("Could not find file contents in SourceArcade response."));
        };

        Ok(html[start..end]
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&#39;", "'")
            .replace("&quot;", "\""))
    }

    #[rhai_fn(return_raw, global)]
    pub fn hex(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = get_json_value(ctx, &format!("https://hex.pm/api/packages/{name}"))?;
        let version = obj
            .get("latest_stable_version")
            .ok_or_else(|| E::from("No json[`latest_stable_version`]?"))?;
        version.as_str().map(str::to_owned).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn opam(ctx: NativeCallContext, name: &str) -> Res<String> {
        let entries = get_json_value(
            ctx,
            &format!("https://api.github.com/repos/ocaml/opam-repository/contents/packages/{name}"),
        )?;
        let entries =
            entries.as_array().ok_or_else(|| E::from("OPAM package listing is not an array"))?;
        let prefix = format!("{name}.");
        let mut latest: Option<(Version, String)> = None;

        for entry in entries {
            let Some(entry_name) = entry["name"].as_str() else { continue };
            let Some(version) = entry_name.strip_prefix(&prefix) else { continue };
            let normalized_version = version.trim_start_matches('v').replace('~', "-");
            let Ok(parsed_version) = Version::parse(&normalized_version) else { continue };

            if latest.as_ref().is_none_or(|(current, _)| parsed_version > *current) {
                latest = Some((parsed_version, version.to_owned()));
            }
        }

        latest
            .map(|(_, version)| version)
            .ok_or_else(|| E::from("No valid OPAM package versions could be found."))
    }

    #[rhai_fn(return_raw, global)]
    pub fn cran(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = get_json_value(ctx, &format!("https://crandb.r-pkg.org/{name}"))?;
        let version = obj.get("Version").ok_or_else(|| E::from("No json[`Version`]?"))?;
        version.as_str().map(str::to_owned).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn pypi(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = get_json_value(ctx, &format!("https://pypi.org/pypi/{name}/json"))?;
        let obj = obj.get("info").ok_or_else(|| E::from("No json[`info`]?"))?;
        let obj = obj.get("version").ok_or_else(|| E::from("No json[`info`][`version`]?"))?;
        obj.as_str().map(str::to_owned).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn crates(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = get_json_value(ctx, &format!("https://crates.io/api/v1/crates/{name}"))?;
        let obj = obj.get("crate").ok_or_else(|| E::from("No json[`crate`]?"))?;
        let obj = obj.get("max_stable_version");
        let obj = obj.ok_or_else(|| E::from("No json[`crate`][`max_stable_version`]?"))?;
        obj.as_str().map(str::to_owned).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn crates_max(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = get_json_value(ctx, &format!("https://crates.io/api/v1/crates/{name}"))?;
        let obj = obj.get("crate").ok_or_else(|| E::from("No json[`crate`]?"))?;
        let obj = obj.get("max_version");
        let obj = obj.ok_or_else(|| E::from("No json[`crate`][`max_version`]?"))?;
        obj.as_str().map(str::to_owned).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn crates_newest(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = get_json_value(ctx, &format!("https://crates.io/api/v1/crates/{name}"))?;
        let obj = obj.get("crate").ok_or_else(|| E::from("No json[`crate`]?"))?;
        let obj = obj.get("newest_version");
        let obj = obj.ok_or_else(|| E::from("No json[`crate`][`newest_version`]?"))?;
        obj.as_str().map(str::to_owned).ok_or_else(|| "json not string?".into())
    }
    #[rhai_fn(return_raw, global)]
    pub fn npm(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = get_json_value(ctx, &format!("https://registry.npmjs.org/{name}/latest"))?;
        let obj = obj.get("version").ok_or_else(|| E::from("No json[`version`]?"))?;
        obj.as_str().map(str::to_owned).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn hackage(ctx: NativeCallContext, name: &str) -> Res<String> {
        let obj = get_json_value(
            ctx,
            &format!("https://hackage.haskell.org/package/{name}/preferred.json"),
        )?;
        let versions =
            obj.get("normal-version").ok_or_else(|| E::from("No json[`normal-version`]"))?;
        let latest = versions
            .as_array()
            .ok_or_else(|| E::from("`normal-version` is not an array"))?
            .first()
            .ok_or_else(|| E::from("No normal package versions available"))?;
        latest.as_str().map(str::to_owned).ok_or_else(|| E::from("Package version is not a string"))
    }

    #[rhai_fn(return_raw, global)]
    pub fn codeberg(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let req = AGENT.get(&format!("https://codeberg.org/api/v1/repos/{repo}/releases/latest"));
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        Ok(v["tag_name"].as_str().unwrap_or("").to_owned())
    }

    #[rhai_fn(return_raw, global)]
    pub fn codeberg_tag(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let req = AGENT.get(&format!("https://codeberg.org/api/v1/repos/{repo}/tags"));
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        let v = (v.as_array())
            .ok_or_else(|| E::from("codeberg_tag received not array"))
            .map(|a| a.first().ok_or_else(|| E::from("codeberg_tag no tags")))??;
        Ok(v["name"].as_str().unwrap_or("").to_owned())
    }

    #[rhai_fn(return_raw, global)]
    pub fn codeberg_commit(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let req = AGENT.get(&format!("https://codeberg.org/api/v1/repos/{repo}/commits?stat=false&verification=false&files=false&limit=1"));
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        Ok(v[0]["sha"].as_str().unwrap_or("").to_owned())
    }

    #[rhai_fn(return_raw, global)]
    pub fn codeberg_rawfile(
        ctx: NativeCallContext,
        repo: &str,
        branch: &str,
        file: &str,
    ) -> Res<String> {
        get(ctx, &format!("https://codeberg.org/{repo}/raw/branch/{branch}/{file}"))
    }

    #[rhai_fn(return_raw, global)]
    pub fn gems(ctx: NativeCallContext, gem: &str) -> Res<String> {
        let obj = get_json_value(
            ctx,
            &format!("https://rubygems.org/api/v1/versions/{gem}/latest.json"),
        )?;
        let obj = obj.get("version").ok_or_else(|| E::from("No json[`version`]?"))?;
        obj.as_str().map(str::to_owned).ok_or_else(|| "json not string?".into())
    }

    #[rhai_fn(return_raw, global)]
    pub fn gitea(ctx: NativeCallContext, host: &str, repo: &str) -> Res<String> {
        let req = AGENT.get(&format!("https://{host}/api/v1/repos/{repo}/releases/latest"));
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        Ok(v["tag_name"].as_str().unwrap_or("").to_owned())
    }

    #[rhai_fn(return_raw, global)]
    pub fn gitea_tag(ctx: NativeCallContext, host: &str, repo: &str) -> Res<String> {
        let req = AGENT.get(&format!("https://{host}/api/v1/repos/{repo}/tags"));
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        let v = (v.as_array())
            .ok_or_else(|| E::from("gitea_tag received not array"))
            .map(|a| a.first().ok_or_else(|| E::from("gitea_tag no tags")))??;
        Ok(v["name"].as_str().unwrap_or("").to_owned())
    }

    #[rhai_fn(return_raw, global)]
    pub fn gitea_commit(ctx: NativeCallContext, host: &str, repo: &str) -> Res<String> {
        let req = AGENT.get(&format!("https://{host}/api/v1/repos/{repo}/commits?limit=1"));
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo}:\n{v}");
        Ok(v[0]["sha"].as_str().unwrap_or("").to_owned())
    }

    #[rhai_fn(skip)]
    fn tangled_remote(ctx: &mut NativeCallContext, repo: &str) -> Result<Remote<'static>, E> {
        let mut remote =
            Remote::create_detached(format!("https://tangled.org/{repo}")).ehdl(ctx)?;
        remote.connect(git2::Direction::Fetch).ehdl(ctx)?;
        Ok(remote)
    }

    #[rhai_fn(return_raw, global)]
    pub fn tangled(mut ctx: NativeCallContext, repo: &str) -> Res<String> {
        let remote = tangled_remote(&mut ctx, repo)?;
        let mut latest = Version::new(0, 0, 0);

        for head in remote.list().ehdl(&ctx)? {
            if head.name().ends_with("^{}") {
                continue;
            }

            let Some(tag_name) = head.name().strip_prefix("refs/tags/") else { continue };
            let Some(version_start_index) = tag_name.find(char::is_numeric) else { continue };
            let (_, version_str) = tag_name.split_at(version_start_index);
            let Ok(parsed_version) = Version::parse(version_str) else { continue };

            if parsed_version > latest {
                latest = parsed_version;
            }
        }

        if latest == Version::new(0, 0, 0) {
            return Err(E::from("No valid version tags could be found."));
        }

        Ok(latest.to_string())
    }

    #[rhai_fn(return_raw, global)]
    pub fn tangled_tag(mut ctx: NativeCallContext, repo: &str) -> Res<String> {
        let remote = tangled_remote(&mut ctx, repo)?;
        let mut latest: Option<(Version, String)> = None;

        for head in remote.list().ehdl(&ctx)? {
            if head.name().ends_with("^{}") {
                continue;
            }

            let Some(tag_name) = head.name().strip_prefix("refs/tags/") else { continue };
            let Some(version_start_index) = tag_name.find(char::is_numeric) else { continue };
            let (_, version_str) = tag_name.split_at(version_start_index);
            let Ok(version) = Version::parse(version_str) else { continue };

            if latest.as_ref().is_none_or(|(current, _)| version > *current) {
                latest = Some((version, tag_name.to_owned()));
            }
        }

        latest.map(|(_, tag)| tag).ok_or_else(|| E::from("No valid version tags could be found."))
    }

    #[rhai_fn(return_raw, global)]
    pub fn tangled_commit(mut ctx: NativeCallContext, repo: &str) -> Res<String> {
        let remote = tangled_remote(&mut ctx, repo)?;
        for head in remote.list().ehdl(&ctx)? {
            if head.name() == "HEAD" {
                return Ok(head.oid().to_string());
            }
        }

        Err(E::from("Could not find HEAD in repository's reference advertisement list."))
    }

    #[rhai_fn(return_raw, global)]
    pub fn tangled_rawfile(
        ctx: NativeCallContext,
        repo: &str,
        branch: &str,
        file: &str,
    ) -> Res<String> {
        get(ctx, &format!("https://tangled.org/{repo}/raw/{branch}/{file}"))
    }

    #[rhai_fn(return_raw, global)]
    pub fn sourcehut(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let mut remote = Remote::create_detached(format!("https://git.sr.ht/{repo}")).ehdl(&ctx)?;
        remote.connect(git2::Direction::Fetch).ehdl(&ctx)?;

        let mut latest = Version::new(0, 0, 0);

        let heads = remote.list().ehdl(&ctx)?;
        for head in heads {
            if head.name().ends_with("^{}") {
                continue;
            }

            // Let's find the version in the tag name...
            let Some(tag_name) = head.name().strip_prefix("refs/tags/") else { continue };
            let Some(version_start_index) = tag_name.find(char::is_numeric) else { continue };
            let (_, version_str) = tag_name.split_at(version_start_index);

            // Let's parse what should be a valid version
            let Ok(parsed_version) = Version::parse(version_str) else { continue };

            if parsed_version > latest {
                latest = parsed_version;
            }
        }

        if latest == Version::new(0, 0, 0) {
            return Err(E::from("No valid version tags could be found."));
        }

        Ok(latest.to_string())
    }

    #[rhai_fn(return_raw, global)]
    pub fn sourcehut_commit(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let mut remote = Remote::create_detached(format!("https://git.sr.ht/{repo}")).ehdl(&ctx)?;
        remote.connect(git2::Direction::Fetch).ehdl(&ctx)?;

        let heads = remote.list().ehdl(&ctx)?;
        for head in heads {
            if head.name() == "HEAD" {
                return Ok(head.oid().to_string());
            }
        }

        Err(E::from("Could not find HEAD in repository's reference advertisement list."))
    }

    #[rhai_fn(return_raw, global)]
    pub fn sourcehut_rawfile(
        ctx: NativeCallContext,
        repo: &str,
        branch: &str,
        file: &str,
    ) -> Res<String> {
        get(ctx, &format!("https://git.sr.ht/{repo}/blob/{branch}/{file}"))
    }

    #[rhai_fn(return_raw, global)]
    pub fn gnome_extensions(ctx: NativeCallContext, uuid: &str) -> Res<String> {
        let response_value = get_json_value(
            ctx,
            &format!("https://extensions.gnome.org/api/v1/extensions/{uuid}/versions/?format=json"),
        )?;
        trace!("Got json from {uuid}:\n{response_value}");

        let results =
            response_value.get("results").ok_or_else(|| E::from("No json[`results`]?"))?;
        let results_arr =
            results.as_array().ok_or_else(|| E::from("json[`results`] is not array type?"))?;

        // There's both the version name and the internal/fallback version.
        // We'll use the internal/fallback version since the version name is optional and not always present.
        let mut latest_version = 0;
        for result in results_arr {
            let Some(result_obj) = result.as_object() else {
                continue;
            };
            let Some(status_value) = result_obj.get("status") else {
                continue;
            };
            let Some(status) = status_value.as_i64() else {
                continue;
            };

            // Is version marked as "Active"?
            if status != 3 {
                continue;
            }

            let Some(version_value) = result_obj.get("version") else {
                continue;
            };
            let Some(version) = version_value.as_i64() else {
                continue;
            };

            if version > latest_version {
                latest_version = version;
            }
        }

        if latest_version == 0 {
            return Err(E::from("No active extension version could be found!"));
        }

        Ok(latest_version.to_string())
    }

    #[rhai_fn(return_raw, global)]
    pub fn ansible_galaxy(
        ctx: NativeCallContext,
        namespace: &str,
        collection: &str,
    ) -> Res<String> {
        let response =
            AGENT.get(&format!("https://galaxy.ansible.com/api/v3/plugin/ansible/content/published/collections/index/{namespace}/{collection}/versions/?limit=1&ordering=-version"))
                .header("User-Agent", USER_AGENT)
                .header("Accept", "application/json")
                .call()
                .ehdl(&ctx)?;
        let response: Value = response.into_body().read_json().ehdl(&ctx)?;
        Ok(response["data"][0]["version"].as_str().unwrap_or_default().to_owned())
    }

    #[rhai_fn(return_raw, global)]
    pub fn forgejo(ctx: NativeCallContext, host: &str, repo: &str) -> Res<String> {
        let req = AGENT.get(&format!("https://{host}/api/v1/repos/{repo}/releases/latest"));
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo} hosted with Forgejo:\n{v}");
        Ok(v["tag_name"].as_str().unwrap_or("").to_owned())
    }

    #[rhai_fn(return_raw, global)]
    pub fn forgejo_tag(ctx: NativeCallContext, host: &str, repo: &str) -> Res<String> {
        let req = AGENT.get(&format!("https://{host}/api/v1/repos/{repo}/tags"));
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo} hosted with Forgejo:\n{v}");
        let v = (v.as_array())
            .ok_or_else(|| E::from("forgejo_tag received not array"))
            .map(|a| a.first().ok_or_else(|| E::from("forgejo_tag no tags")))??;
        Ok(v["name"].as_str().unwrap_or("").to_owned())
    }

    #[rhai_fn(return_raw, global)]
    pub fn forgejo_commit(ctx: NativeCallContext, host: &str, repo: &str) -> Res<String> {
        let req = AGENT.get(&format!("https://{host}/api/v1/repos/{repo}/commits?limit=1"));
        let v: Value = req.call().ehdl(&ctx)?.into_body().read_json().ehdl(&ctx)?;
        trace!("Got json from {repo} hosted with Forgejo:\n{v}");
        Ok(v[0]["sha"].as_str().unwrap_or("").to_owned())
    }

    /// All tags of an OCI image, e.g. `oci_tags("ghcr.io/org/name")`.
    #[rhai_fn(return_raw, global)]
    pub fn oci_tags(ctx: NativeCallContext, image: &str) -> Res<rhai::Array> {
        let image = oci::ImageRef::parse(image).ehdl(&ctx)?;
        let tags = oci::tags(&AGENT, &image).ehdl(&ctx)?;
        Ok(tags.into_iter().map(rhai::Dynamic::from).collect())
    }

    /// Newest release tag of an OCI image, e.g. `1.2.3`, `v0.10.0`, `17.6` or `2024.01.05`.
    #[rhai_fn(return_raw, global)]
    pub fn oci_tag(ctx: NativeCallContext, image: &str) -> Res<String> {
        let image = oci::ImageRef::parse(image).ehdl(&ctx)?;
        oci::newest_tag(&AGENT, &image).ehdl(&ctx)
    }

    /// `sha256:…` digest of an OCI image tag; the index digest for multi-arch images.
    #[rhai_fn(return_raw, global)]
    pub fn oci_digest(ctx: NativeCallContext, image: &str, tag: &str) -> Res<String> {
        let image = oci::ImageRef::parse(image).ehdl(&ctx)?;
        oci::digest(&AGENT, &image, tag).ehdl(&ctx)
    }

    #[rhai_fn(return_raw, global)]
    pub fn git_tags(ctx: NativeCallContext, url: &str) -> Res<rhai::Array> {
        const TAGS_PREFIX: &str = "refs/tags/";
        const RESOLVED_SUFFIX: &str = "^{}";

        let mut tags = Vec::new();

        let req = (AGENT.get(&format!("{url}/info/refs?service=git-upload-pack")))
            .header("Content-Type", "application/x-git-upload-pack-request");

        let resp = req.call().ehdl(&ctx)?;
        let is_smart = resp.headers().iter().any(|(key, value)| {
            key == "Content-Type" && value == "application/x-git-upload-pack-advertisement"
        });
        let body: String = resp.into_body().read_to_string().ehdl(&ctx)?;

        for line in body.lines().map(str::trim).filter(|a| !a.is_empty() && !a.starts_with('#')) {
            let name = if is_smart {
                let line_length = usize::from_str_radix(&line[..4], 16).unwrap_or(0).checked_sub(1);
                let Some(line_length) = line_length else {
                    continue;
                };
                line[4..line_length].split_once(' ').map_or("", |(_ref, name)| name)
            } else {
                line.split_once('\t').map_or("", |(_ref, name)| name)
            };

            if !name.starts_with(TAGS_PREFIX) || name.ends_with(RESOLVED_SUFFIX) {
                continue;
            }

            tags.push(name[TAGS_PREFIX.len()..].to_owned());
        }

        sort_git_tags_impl(&mut tags);
        Ok(tags.into_iter().map(rhai::Dynamic::from).collect())
    }

    #[rhai_fn(skip)]
    pub fn internal_env(key: &str) -> Res<String> {
        trace!("env(`{key}`) = {:?}", std::env::var(key));
        match std::env::var(key) {
            Ok(s) => Ok(s),
            Err(VarError::NotPresent) => Err(format!("env(`{key}`) not present").into()),
            Err(VarError::NotUnicode(o)) => {
                Err(format!("env(`{key}`): invalid UTF: {}", o.display()).into())
            }
        }
    }

    #[rhai_fn(global)]
    pub fn env(key: &str) -> String {
        trace!("env(`{key}`) = {:?}", std::env::var_os(key));
        std::env::var_os(key).map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    }
}

#[derive(Clone)]
pub struct Req {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub redirects: i64,
}

impl CustomType for Req {
    fn build(mut builder: rhai::TypeBuilder<'_, Self>) {
        builder
            .with_name("Req")
            .with_fn("new_req", Self::new)
            .with_fn("get", |ctx: NativeCallContext, x: Self| rf(&ctx, x.get()))
            .with_fn("redirects", Self::redirects)
            .with_fn("head", Self::head);
    }
}

impl Req {
    pub const fn new(url: String) -> Self {
        Self { url, headers: vec![], redirects: 0 }
    }
    pub fn get(self) -> color_eyre::Result<String> {
        let cfg = ureq::Agent::config_builder().max_redirects(self.redirects.try_into()?).build();
        let r = ureq::Agent::new_with_config(cfg).get(&self.url);
        let mut r = r.header("User-Agent", USER_AGENT);
        for (k, v) in self.headers {
            r = r.header(k.as_str(), v.as_str());
        }
        Ok(r.call()?.into_body().read_to_string()?)
    }
    pub fn head(&mut self, key: String, val: String) {
        self.headers.push((key, val));
    }
    pub const fn redirects(&mut self, i: i64) {
        self.redirects = i;
    }
}

#[cfg(test)]
mod tests {
    use super::sort_git_tags_impl;

    #[test]
    fn sorts_version_tags_semantically() {
        let mut tags = vec![
            "1.9.3".to_owned(),
            "1.13.5".to_owned(),
            "1.10.0".to_owned(),
            "v2.0.0".to_owned(),
            "nightly".to_owned(),
            "ensu-v0.1.9".to_owned(),
            "ensu-v0.1.19".to_owned(),
            "ensu2-v0.1.19-beta".to_owned(),
        ];

        sort_git_tags_impl(&mut tags);

        assert_eq!(
            tags,
            [
                "nightly",
                "ensu-v0.1.9",
                "ensu2-v0.1.19-beta",
                "ensu-v0.1.19",
                "1.9.3",
                "1.10.0",
                "1.13.5",
                "v2.0.0"
            ]
        );
    }

    #[test]
    fn sorts_prereleases_before_releases() {
        let mut tags = vec!["1.13.5".to_owned(), "1.13.5-rc.1".to_owned(), "1.13.4".to_owned()];

        sort_git_tags_impl(&mut tags);

        assert_eq!(tags, ["1.13.4", "1.13.5-rc.1", "1.13.5"]);
    }
}
