use color_eyre::{
    eyre::{bail, eyre},
    Result,
};
use std::{collections::HashMap, path::Path, process::Command};
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
    let urls = gitmodules_paths_to_urls(gitmodules)?;
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
    Ok(submodules)
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
    fn names_globals_after_the_whole_path() {
        assert_eq!(
            global_name("lib/twitch-eventsub-ws/lib/date"),
            "submodule_lib_twitch_eventsub_ws_lib_date"
        );
    }
}
