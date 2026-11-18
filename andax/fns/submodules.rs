use color_eyre::{
    eyre::{bail, eyre},
    Result,
};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    process::Command,
};
use tracing::warn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submodule {
    pub path: String,
    pub url: String,
    pub commit: String,
}

impl Submodule {
    pub fn into_map(self) -> rhai::Map {
        rhai::Map::from([
            ("path".into(), self.path.into()),
            ("url".into(), self.url.into()),
            ("commit".into(), self.commit.into()),
        ])
    }

    pub fn from_map(map: &rhai::Map) -> Result<Self> {
        let field = |key: &str| {
            map.get(key)
                .and_then(|value| value.clone().into_string().ok())
                .ok_or_else(|| eyre!("submodule entry is missing a `{key}` string: {map:?}"))
        };
        Ok(Self { path: field("path")?, url: field("url")?, commit: field("commit")? })
    }
}

pub struct Gitlink {
    pub path: String,
    pub commit: String,
}

pub fn resolve_url(superproject: &str, url: &str) -> String {
    if !url.starts_with("./") && !url.starts_with("../") {
        return url.to_owned();
    }
    let mut base = superproject.trim_end_matches('/').to_owned();
    let mut rest = url;
    loop {
        if let Some(after) = rest.strip_prefix("./") {
            rest = after;
        } else if let Some(after) = rest.strip_prefix("../") {
            rest = after;
            if let Some(slash) = base.rfind('/') {
                base.truncate(slash);
            }
        } else {
            break;
        }
    }
    format!("{base}/{rest}")
}

fn gitmodules_paths_to_urls(gitmodules: &str) -> Result<HashMap<String, String>> {
    let file = tempfile::NamedTempFile::new()?;
    std::fs::write(file.path(), gitmodules)?;
    let config = git2::Config::open(file.path())?;

    let mut paths = HashMap::new();
    let mut urls = HashMap::new();
    let mut entries = config.entries(Some(r"^submodule\..*\.(path|url)$"))?;
    while let Some(entry) = entries.next() {
        let entry = entry?;
        let (Ok(key), Ok(value)) = (entry.name(), entry.value()) else { continue };
        let Some(key) = key.strip_prefix("submodule.") else { continue };
        if let Some(name) = key.strip_suffix(".path") {
            paths.insert(name.to_owned(), value.to_owned());
        } else if let Some(name) = key.strip_suffix(".url") {
            urls.insert(name.to_owned(), value.to_owned());
        }
    }
    Ok(paths.into_iter().filter_map(|(name, path)| Some((path, urls.remove(&name)?))).collect())
}

pub fn join(
    superproject: &str,
    gitlinks: Vec<Gitlink>,
    gitmodules: &str,
) -> Result<Vec<Submodule>> {
    Ok(join_urls(superproject, gitlinks, &gitmodules_paths_to_urls(gitmodules)?))
}

fn join_urls(
    superproject: &str,
    gitlinks: Vec<Gitlink>,
    urls: &HashMap<String, String>,
) -> Vec<Submodule> {
    let mut submodules: Vec<Submodule> = gitlinks
        .into_iter()
        .filter_map(|Gitlink { path, commit }| {
            let Some(url) = urls.get(&path) else {
                warn!("{superproject}: `{path}` is a gitlink without a .gitmodules entry, skipping it");
                return None;
            };
            Some(Submodule { url: resolve_url(superproject, url), path, commit })
        })
        .collect();
    submodules.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    submodules
}

fn split_parent(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

fn from_directory_listings(
    superproject: &str,
    gitmodules: &str,
    mut list_submodules_in: impl FnMut(&str) -> Result<HashMap<String, String>>,
) -> Result<Vec<Submodule>> {
    let urls = gitmodules_paths_to_urls(gitmodules)?;
    let mut by_dir: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for path in urls.keys() {
        by_dir.entry(split_parent(path).0).or_default().push(path);
    }

    let mut gitlinks = vec![];
    for (dir, paths) in by_dir {
        let listed = list_submodules_in(dir)?;
        for path in paths {
            if let Some(commit) = listed.get(split_parent(path).1) {
                gitlinks.push(Gitlink { path: path.to_owned(), commit: commit.clone() });
            } else {
                warn!(
                    "{superproject}: .gitmodules lists `{path}`, but it isn't a submodule at this ref, skipping it"
                );
            }
        }
    }
    Ok(join_urls(superproject, gitlinks, &urls))
}

const PATH_SEGMENT: &AsciiSet =
    &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

fn encode_component(component: &str) -> String {
    utf8_percent_encode(component, PATH_SEGMENT).to_string()
}

fn encode_path(path: &str) -> String {
    path.split('/').map(encode_component).collect::<Vec<_>>().join("/")
}

fn optional_text(agent: &ureq::Agent, url: &str, query: &[(&str, &str)]) -> Result<Option<String>> {
    let response = agent
        .get(url)
        .query_pairs(query.iter().copied())
        .header("User-Agent", super::tsunagu::USER_AGENT)
        .call();
    match response {
        Ok(response) => {
            let text = response.into_body().read_to_string()?;
            Ok((!text.trim().is_empty()).then_some(text))
        }
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(e) => Err(eyre!("{url}: {e}")),
    }
}

fn get_json(
    agent: &ureq::Agent,
    url: &str,
    query: &[(&str, &str)],
) -> Result<(Value, Option<String>)> {
    let response = agent
        .get(url)
        .query_pairs(query.iter().copied())
        .header("User-Agent", super::tsunagu::USER_AGENT)
        .call()
        .map_err(|e| eyre!("{url}: {e}"))?;
    let next_page = response
        .headers()
        .get("x-next-page")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Ok((response.into_body().read_json()?, next_page))
}

fn submodule_entries(
    listing: &Value,
    kind: &str,
    commit_key: &str,
) -> Result<HashMap<String, String>> {
    Ok(listing
        .as_array()
        .ok_or_else(|| eyre!("expected a directory listing, got {listing}"))?
        .iter()
        .filter(|entry| entry["type"] == kind)
        .filter_map(|entry| {
            Some((entry["name"].as_str()?.to_owned(), entry[commit_key].as_str()?.to_owned()))
        })
        .collect())
}

pub fn from_gitlab(
    agent: &ureq::Agent,
    domain: &str,
    id: &str,
    git_ref: &str,
) -> Result<Vec<Submodule>> {
    let api = format!("https://{domain}/api/v4/projects/{}", encode_component(id));
    let Some(gitmodules) = optional_text(
        agent,
        &format!("{api}/repository/files/.gitmodules/raw"),
        &[("ref", git_ref)],
    )?
    else {
        return Ok(vec![]);
    };
    let superproject = if id.contains('/') {
        format!("https://{domain}/{id}")
    } else {
        let (project, _) = get_json(agent, &api, &[])?;
        project["web_url"]
            .as_str()
            .ok_or_else(|| eyre!("{api}: project has no web_url"))?
            .to_owned()
    };

    from_directory_listings(&superproject, &gitmodules, |dir| {
        let mut entries = HashMap::new();
        let mut page = "1".to_owned();
        loop {
            let mut query = vec![("ref", git_ref), ("per_page", "100"), ("page", page.as_str())];
            if !dir.is_empty() {
                query.push(("path", dir));
            }
            let (listing, next_page) = get_json(agent, &format!("{api}/repository/tree"), &query)?;
            entries.extend(submodule_entries(&listing, "commit", "id")?);
            match next_page {
                Some(next) => page = next,
                None => return Ok(entries),
            }
        }
    })
}

pub fn from_gitea(
    agent: &ureq::Agent,
    host: &str,
    repo: &str,
    git_ref: &str,
) -> Result<Vec<Submodule>> {
    let api = format!("https://{host}/api/v1/repos/{repo}");
    let Some(gitmodules) =
        optional_text(agent, &format!("{api}/raw/.gitmodules"), &[("ref", git_ref)])?
    else {
        return Ok(vec![]);
    };
    from_directory_listings(&format!("https://{host}/{repo}"), &gitmodules, |dir| {
        let url = if dir.is_empty() {
            format!("{api}/contents")
        } else {
            format!("{api}/contents/{}", encode_path(dir))
        };
        let (listing, _) = get_json(agent, &url, &[("ref", git_ref)])?;
        submodule_entries(&listing, "submodule", "sha")
    })
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| eyre!("can't run git: {e}"))?;
    if !output.status.success() {
        bail!("git {}: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub fn from_git(url: &str, git_ref: &str) -> Result<Vec<Submodule>> {
    let dir = tempfile::tempdir()?;
    let dir = dir.path();
    git(dir, &["init", "-q"])?;
    git(dir, &["remote", "add", "origin", url])?;
    git(
        dir,
        &["fetch", "-q", "--depth", "1", "--filter=blob:none", "--no-tags", "origin", git_ref],
    )?;

    let gitlinks: Vec<Gitlink> = git(dir, &["ls-tree", "-r", "-z", "FETCH_HEAD"])?
        .split('\0')
        .filter_map(|entry| {
            let (meta, path) = entry.split_once('\t')?;
            let mut meta = meta.split(' ');
            (meta.next()? == "160000").then_some(())?;
            let commit = meta.nth(1)?;
            Some(Gitlink { path: path.to_owned(), commit: commit.to_owned() })
        })
        .collect();
    if gitlinks.is_empty() {
        return Ok(vec![]);
    }
    let gitmodules = git(dir, &["show", "FETCH_HEAD:.gitmodules"])?;
    join(url, gitlinks, &gitmodules)
}

pub fn global_name(path: &str) -> String {
    let sanitized: String =
        path.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("submodule_{sanitized}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_urls_against_the_superproject() {
        let base = "https://github.com/org/app.git";
        assert_eq!(resolve_url(base, "../lib.git"), "https://github.com/org/lib.git");
        assert_eq!(resolve_url(base, "./vendored"), "https://github.com/org/app.git/vendored");
        assert_eq!(resolve_url(base, "../../other/lib"), "https://github.com/other/lib");
        assert_eq!(resolve_url(base, "https://gitlab.com/x/y"), "https://gitlab.com/x/y");
    }

    #[test]
    fn joins_gitlinks_with_gitmodules_by_path() {
        let gitmodules = "[submodule \"magic\"]\n\tpath = lib/magic_enum\n\turl = https://github.com/Neargye/magic_enum\n\
            # a comment\n[submodule \"rel\"]\n    path = third_party/rel\n    url = ../rel.git\n";
        let gitlinks = vec![
            Gitlink { path: "third_party/rel".into(), commit: "b".repeat(40) },
            Gitlink { path: "lib/magic_enum".into(), commit: "a".repeat(40) },
            Gitlink { path: "orphan".into(), commit: "c".repeat(40) },
        ];
        let submodules = join("https://github.com/org/app", gitlinks, gitmodules).unwrap();
        assert_eq!(
            submodules,
            vec![
                Submodule {
                    path: "lib/magic_enum".into(),
                    url: "https://github.com/Neargye/magic_enum".into(),
                    commit: "a".repeat(40),
                },
                Submodule {
                    path: "third_party/rel".into(),
                    url: "https://github.com/org/rel.git".into(),
                    commit: "b".repeat(40),
                },
            ]
        );
    }

    #[test]
    fn matches_gitmodules_against_parent_directory_listings() {
        let gitmodules = "[submodule \"a\"]\n\tpath = external/a\n\turl = ../a.git\n\
            [submodule \"b\"]\n\tpath = external/b\n\turl = https://example.com/b\n\
            [submodule \"top\"]\n\tpath = top\n\turl = https://example.com/top\n\
            [submodule \"stale\"]\n\tpath = external/stale\n\turl = https://example.com/stale\n";
        let mut listed_dirs = vec![];
        let submodules =
            from_directory_listings("https://codeberg.org/org/app", gitmodules, |dir| {
                listed_dirs.push(dir.to_owned());
                Ok(match dir {
                    "external" => {
                        HashMap::from([("a".into(), "1".repeat(40)), ("b".into(), "2".repeat(40))])
                    }
                    "" => HashMap::from([("top".into(), "3".repeat(40))]),
                    _ => HashMap::new(),
                })
            })
            .unwrap();
        assert_eq!(listed_dirs, vec!["", "external"]);
        assert_eq!(
            submodules.iter().map(|s| (s.path.as_str(), s.url.as_str())).collect::<Vec<_>>(),
            vec![
                ("external/a", "https://codeberg.org/org/a.git"),
                ("external/b", "https://example.com/b"),
                ("top", "https://example.com/top"),
            ]
        );
    }

    #[test]
    fn encodes_api_path_segments() {
        assert_eq!(encode_component("group/sub/project"), "group%2Fsub%2Fproject");
        assert_eq!(encode_path("third party/lib-a.v2"), "third%20party/lib-a.v2");
    }

    #[test]
    fn names_globals_after_the_whole_path() {
        assert_eq!(
            global_name("lib/twitch-eventsub-ws/lib/date"),
            "submodule_lib_twitch_eventsub_ws_lib_date"
        );
    }
}
