use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tracing::{debug, instrument, trace};

use crate::error::ProjectError;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde_with::skip_serializing_none]
pub struct ProjectData {
    pub manifest: HashMap<String, String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde_with::skip_serializing_none]
pub struct Manifest {
    pub project: BTreeMap<String, Project>,
    #[serde(default)]
    pub config: Config,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde_with::skip_serializing_none]
pub struct Config {
    pub mock_config: Option<String>,
    pub strip_prefix: Option<String>,
    pub strip_suffix: Option<String>,
    pub project_regex: Option<String>,
}

impl Manifest {
    #[must_use]
    pub fn find_key_for_value(&self, value: &Project) -> Option<&String> {
        self.project.iter().find_map(|(key, val)| (val == value).then_some(key))
    }

    #[must_use]
    pub fn get_project(&self, key: &str) -> Option<&Project> {
        self.project.get(key).map_or_else(
            || {
                self.project.iter().find_map(|(_k, v)| {
                    let alias = v.alias.as_ref()?;
                    alias.iter().any(|alias| alias == key).then_some(v)
                })
            },
            Some,
        )
    }
}

#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone, Default)]
#[serde_with::skip_serializing_none]
pub struct Project {
    pub rpm: Option<RpmBuild>,
    pub podman: Option<Docker>,
    pub docker: Option<Docker>,
    pub flatpak: Option<Flatpak>,
    pub pre_script: Option<PathBuf>,
    pub post_script: Option<PathBuf>,
    pub env: Option<BTreeMap<String, String>>,
    pub alias: Option<Vec<String>>,
    pub scripts: Option<Vec<PathBuf>>,
    #[serde(default)]
    #[serde(deserialize_with = "btree_wild_string")]
    pub labels: BTreeMap<String, String>,
    pub update: Option<PathBuf>,
    pub arches: Option<Vec<String>>,
}

/// Deserialize the value of the BTreeMap into a String even if they are some other types.
///
/// # Errors
/// This function itself does not raise any errors unless the given value has the wrong type.
/// However, it inherits errors from `serde::Deserializer`.
fn btree_wild_string<'de, D>(deserializer: D) -> Result<BTreeMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct WildString;

    impl serde::de::Visitor<'_> for WildString {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string, integer, bool or unit")
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v)
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v.to_owned())
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(format!("{v}"))
        }

        fn visit_i128<E>(self, v: i128) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(format!("{v}"))
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(format!("{v}"))
        }

        fn visit_u128<E>(self, v: u128) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(format!("{v}"))
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(String::new())
        }

        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(format!("{v}"))
        }
    }

    struct RealWildString(String);

    impl<'de> Deserialize<'de> for RealWildString {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(WildString).map(Self)
        }
    }

    struct BTreeWildStringVisitor;

    impl<'de> serde::de::Visitor<'de> for BTreeWildStringVisitor {
        type Value = BTreeMap<String, String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("map (key: string, value: wild string)")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut res = Self::Value::new();
            while let Some((k, v)) = map.next_entry::<String, RealWildString>()? {
                res.insert(k, v.0);
            }
            Ok(res)
        }
    }

    deserializer.deserialize_map(BTreeWildStringVisitor)
}

#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone, Default)]
#[serde_with::skip_serializing_none]
pub struct RpmBuild {
    pub spec: PathBuf,
    pub sources: Option<PathBuf>,
    pub package: Option<String>,
    pub pre_script: Option<PathBuf>,
    pub post_script: Option<PathBuf>,
    pub enable_scm: Option<bool>,
    #[serde(default)]
    pub extra_repos: Vec<String>,
    pub scm_opts: Option<BTreeMap<String, String>>,
    pub config: Option<BTreeMap<String, String>>,
    pub mock_config: Option<String>,
    pub plugin_opts: Option<BTreeMap<String, String>>,
    pub macros: Option<BTreeMap<String, String>>,
    pub opts: Option<BTreeMap<String, String>>,
}

#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone, Default)]
#[serde_with::skip_serializing_none]
pub struct Docker {
    pub image: BTreeMap<String, DockerImage>, // tag, file
}

pub fn parse_kv(input: &str) -> impl Iterator<Item = Option<(String, String)>> + '_ {
    input
        .split(',')
        .filter(|item| !item.trim().is_empty())
        .map(|item| item.split_once('=').map(|(l, r)| (l.to_owned(), r.to_owned())))
}

pub fn parse_filters(filters: &[String]) -> Option<Vec<Vec<(String, String)>>> {
    filters.iter().map(std::ops::Deref::deref).map(crate::parse_kv).map(Iterator::collect).collect()
}

/// Turn a string into a BTreeMap<String, String>
pub fn parse_labels<'a, I: Iterator<Item = &'a str>>(labels: I) -> Option<Vec<(String, String)>> {
    labels.flat_map(parse_kv).collect()
}

#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone, Default)]
#[serde_with::skip_serializing_none]
pub struct DockerImage {
    pub dockerfile: Option<String>,
    pub import: Option<PathBuf>,
    pub tag_latest: Option<bool>,
    pub context: String,
    pub version: Option<String>,
}

#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone)]
#[serde_with::skip_serializing_none]
pub struct Flatpak {
    pub manifest: PathBuf,
    pub pre_script: Option<PathBuf>,
    pub post_script: Option<PathBuf>,
}

/// Converts a [`Manifest`] to `String` (.hcl).
///
/// # Errors
/// - [`hcl::Error`] : Cannot convert to HCL.
pub fn to_string(config: &Manifest) -> Result<String, hcl::Error> {
    let config = hcl::to_string(&config)?;
    Ok(config)
}

///
/// # Errors
/// Returns an error when the manifest or one of its nested configurations cannot be read or parsed.
#[instrument]
pub fn load_from_file(path: &PathBuf) -> Result<Manifest, ProjectError> {
    debug!("Reading hcl file: {path:?}");
    let file = fs::read_to_string(path).map_err(|e| match e.kind() {
        ErrorKind::NotFound => ProjectError::NoManifest,
        _ => ProjectError::InvalidManifest(e.to_string()),
    })?;

    debug!("Loading config from {path:?}");
    let mut config = load_from_string(&file)?;

    // recursively merge configs

    // get parent path of config file
    let parent = if path.parent().unwrap_or_else(|| Path::new(".")).as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
    };

    let walk = ignore::Walk::new(parent);

    let path = path.canonicalize().map_err(|e| ProjectError::InvalidManifest(e.to_string()))?;

    for entry in walk {
        trace!("Found {entry:?}");
        let entry = entry.map_err(|e| ProjectError::InvalidManifest(e.to_string()))?;

        if entry.file_type().is_some_and(|kind| kind.is_file())
            && entry.path().file_name().is_some_and(|name| name == "anda.hcl")
        {
            // Walk paths may be relative, unlike the canonical root path. Only
            // canonicalize manifest candidates, and never parse the root twice.
            if entry
                .path()
                .canonicalize()
                .map_err(|e| ProjectError::InvalidManifest(e.to_string()))?
                == path
            {
                continue;
            }
            debug!("Loading: {entry:?}");
            let readfile = fs::read_to_string(entry.path())
                .map_err(|e| ProjectError::InvalidManifest(e.to_string()))?;

            let en = entry.path().parent().unwrap_or_else(|| Path::new("."));

            let nested_config = prefix_config(
                load_from_string(&readfile)?,
                &en.strip_prefix("./").unwrap_or(en).display().to_string(),
            );
            // merge the btreemap
            config.project.extend(nested_config.project);
        }
    }

    trace!("Loaded config: {config:#?}");
    generate_alias(&mut config);

    check_config(config)
}

#[must_use]
pub fn prefix_config(mut config: Manifest, prefix: &str) -> Manifest {
    let projects = std::mem::take(&mut config.project);

    for (project_name, mut new_project) in projects {
        // set project name to prefix
        let new_project_name = format!("{prefix}/{project_name}");
        macro_rules! default {
            ($o:expr, $attr:ident, $d:expr) => {
                if let Some($attr) = &mut $o.$attr {
                    if $attr.as_os_str().is_empty() {
                        *$attr = $d.into();
                    }
                    *$attr = PathBuf::from(format!("{prefix}/{}", $attr.display()));
                } else {
                    let p = PathBuf::from(format!("{prefix}/{}", $d));
                    if p.exists() {
                        $o.$attr = Some(p);
                    }
                }
            };
        } // default!(obj, attr, default_value);
        if let Some(rpm) = &mut new_project.rpm {
            rpm.spec = PathBuf::from(format!("{prefix}/{}", rpm.spec.display()));
            default!(rpm, pre_script, "rpm_pre.rhai");
            default!(rpm, post_script, "rpm_post.rhai");
            default!(rpm, sources, ".");
        }
        default!(new_project, update, "update.rhai");
        default!(new_project, pre_script, "pre.rhai");
        default!(new_project, post_script, "post.rhai");

        if let Some(scripts) = &mut new_project.scripts {
            for scr in scripts {
                *scr = PathBuf::from(format!("{prefix}/{}", scr.display()));
            }
        }

        config.project.insert(new_project_name, new_project);
    }
    generate_alias(&mut config);
    config
}

pub fn generate_alias(config: &mut Manifest) {
    fn append_vec(vec: &mut Option<Vec<String>>, value: String) {
        if let Some(vec) = vec {
            if vec.contains(&value) {
                return;
            }

            vec.push(value);
        } else {
            *vec = Some(vec![value]);
        }
    }

    for (name, project) in &mut config.project {
        #[allow(clippy::assigning_clones)]
        if config.config.strip_prefix.is_some() || config.config.strip_suffix.is_some() {
            let mut new_name = name.clone();
            if let Some(strip_prefix) = &config.config.strip_prefix {
                new_name = new_name.strip_prefix(strip_prefix).unwrap_or(&new_name).to_owned();
            }
            if let Some(strip_suffix) = &config.config.strip_suffix {
                new_name = new_name.strip_suffix(strip_suffix).unwrap_or(&new_name).to_owned();
            }

            if name != &new_name {
                append_vec(&mut project.alias, new_name);
            }
        }
    }
}

///
/// # Errors
/// Returns an error when the HCL configuration cannot be parsed.
#[instrument]
pub fn load_from_string(config: &str) -> Result<Manifest, ProjectError> {
    trace!(config, "Dump config");
    let mut config: Manifest = hcl::eval::from_str(config, crate::context::shared_hcl_context())?;

    generate_alias(&mut config);

    check_config(config)
}

/// Lints and checks the config for errors.
///
/// # Errors
/// - nothing. This function literally does nothing. For now.
pub const fn check_config(config: Manifest) -> Result<Manifest, ProjectError> {
    // do nothing for now
    Ok(config)
}

#[allow(clippy::indexing_slicing)]
#[cfg(test)]
mod test_parser {
    use super::*;

    #[test]
    fn test_parse() {
        let config = r#"
        hello = "world"
        project "anda" {
            pre_script {
                commands = [
                    "echo 'parser test'",
                ]
            }
            labels {
                nightly = 1
            }
        }
        "#;

        let body = hcl::parse(config).unwrap();

        print!("{body:#?}");

        let config = load_from_string(config).unwrap();

        println!("{config:#?}");

        assert_eq!(config.project["anda"].labels.get("nightly"), Some(&"1".to_owned()));
    }

    #[test]
    fn prefix_moves_projects_without_losing_configuration() {
        let config = load_from_string(
            r#"
            config { strip_prefix = "anda/" }
            project "pkg" {
                rpm {
                    spec = "example.spec"
                    sources = "sources"
                }
                labels { example = 1 }
            }
        "#,
        )
        .unwrap();
        let prefixed = prefix_config(config, "anda/example");
        assert_eq!(prefixed.project.len(), 1);
        assert_eq!(prefixed.config.strip_prefix.as_deref(), Some("anda/"));
        let project = &prefixed.project["anda/example/pkg"];
        assert_eq!(project.labels.get("example").map(String::as_str), Some("1"));
        let rpm = project.rpm.as_ref().unwrap();
        assert_eq!(rpm.spec, PathBuf::from("anda/example/example.spec"));
        assert_eq!(rpm.sources, Some(PathBuf::from("anda/example/sources")));
    }

    #[test]
    fn root_manifest_is_not_loaded_again_as_a_nested_project() {
        let directory = std::env::temp_dir().join(format!(
            "anda-root-manifest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("anda.hcl");
        fs::write(&path, "project root {}\n").unwrap();
        let loaded = load_from_file(&path).unwrap();
        fs::remove_dir_all(directory).unwrap();
        assert_eq!(loaded.project.len(), 1);
        assert!(loaded.project.contains_key("root"));
    }

    #[test]
    fn relative_root_manifest_is_not_loaded_twice() {
        // Create under the existing cwd, then pass a relative path. Never
        // mutate process-global cwd while the test suite runs concurrently.
        let directory = PathBuf::from(format!(
            ".anda-relative-root-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("anda.hcl");
        fs::write(&path, "project root {}\n").unwrap();
        let loaded = load_from_file(&path);
        fs::remove_dir_all(directory).unwrap();
        let loaded = loaded.unwrap();
        assert_eq!(loaded.project.len(), 1);
        assert!(loaded.project.contains_key("root"));
    }

    #[test]
    fn test_map() {
        let m = [("foo".to_owned(), "bar".to_owned())].into();

        assert_eq!(parse_labels(std::iter::once("foo=bar")), Some(m));

        let multieq = [("foo".to_owned(), "bar=baz".to_owned())].into();

        assert_eq!(parse_labels(std::iter::once("foo=bar=baz")), Some(multieq));

        let multi =
            [("foo".to_owned(), "bar".to_owned()), ("baz".to_owned(), "qux".to_owned())].into();

        assert_eq!(parse_labels(std::iter::once("foo=bar,baz=qux")), Some(multi));
    }
}
