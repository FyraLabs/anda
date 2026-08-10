use crate::{error::AndaxRes, run::rf};
use core::str::FromStr;
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

pub const USER_AGENT: &str = "AndaX";
#[export_module]
pub mod ar {
    type E = Box<rhai::EvalAltResult>;

    static AGENT: std::sync::LazyLock<ureq::Agent> = std::sync::LazyLock::new(|| {
        ureq::Agent::new_with_config(ureq::Agent::config_builder().build())
    });

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
    pub fn gh_tag(ctx: NativeCallContext, repo: &str) -> Res<String> {
        let req = (AGENT.get(&format!("https://api.github.com/repos/{repo}/tags")))
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
        let start = pre_start + content_offset + 1;
        let Some(end) = html[start..].find("</pre>") else {
            return Err(E::from("Could not find file contents in SourceArcade response."));
        };

        Ok(html[start..start + end]
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

    #[rhai_fn(return_raw, global)]
    pub fn git_tags(ctx: NativeCallContext, url: &str) -> Res<rhai::Array> {
        const TAGS_PREFIX: &str = "refs/tags/";
        const RESOLVED_SUFFIX: &str = "^{}";

        let mut tags = rhai::Array::new();

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

            if let Ok(tag) = rhai::Dynamic::from_str(&name[TAGS_PREFIX.len()..]) {
                tags.push(tag);
            }
        }

        Ok(tags)
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
