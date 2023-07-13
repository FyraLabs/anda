//! Parser for rpmspec. See [`SpecParser`].
use crate::error::ParserError;
use crate::util::{gen_read_helper, Consumer, SpecMacroParserIter};
use color_eyre::{
	eyre::{eyre, Context},
	Help, Result, SectionExt,
};
use parking_lot::Mutex;
use regex::Regex;
use smartstring::alias::String;
use std::{
	collections::HashMap,
	io::{BufReader, Read},
	mem::take,
	num::ParseIntError,
	process::Command,
	sync::Arc,
};
use tracing::{debug, error, trace, warn};

const PKGNAMECHARSET: &str = "_-";

lazy_static::lazy_static! {
	static ref RE_PKGQCOND: Regex = Regex::new(r"\s+(>=?|<=?|=)\s+(\d+:)?([\w\d.^~]+)-([\w\d.^~]+)(.*)").unwrap();
	static ref RE_REQ1: Regex = Regex::new(r"(?m)^Requires(?:\(([\w,\s]+)\))?:\s*(.+)$").unwrap();
	static ref RE_REQ2: Regex = Regex::new(r"(?m)([\w-]+)(?:\s*([>=<]{1,2})\s*([\d._~^]+))?").unwrap();
	static ref RE_FILE: Regex = Regex::new(r"(?m)^(%\w+(\(.+\))?\s+)?(.+)$").unwrap();
	static ref RE_CHANGELOG: Regex = Regex::new(r"(?m)^\*[ \t]*((\w{3})[ \t]+(\w{3})[ \t]+(\d+)[ \t]+(\d+))[ \t]+(\S+)([ \t]+<([\w@.+]+)>)?([ \t]+-[ \t]+([\d.-^~_\w]+))?$((\n^[^*\n]*)+)").unwrap();
	static ref RE_PREAMBLE: Regex = Regex::new(r"(\w+):\s*(.+)").unwrap();
	static ref RE_DNL: Regex = Regex::new(r"^%dnl\b").unwrap();
	static ref RE_DIGIT: Regex = Regex::new(r"\d+$").unwrap();
}

/// Conditional operators used in specifying dependencies.
/// ## Symbols
/// - [`PkgQCond::Eq`] : `=`
/// - [`PkgQCond::Le`] : `<=`
/// - [`PkgQCond::Lt`] : `<`
/// - [`PkgQCond::Ge`] : `>=`
/// - [`PkgQCond::Gt`] : `>`
#[derive(Default, Clone, Copy, Debug)]
pub enum PkgQCond {
	/// =
	#[default]
	Eq,
	/// <=
	Le,
	/// <
	Lt,
	/// >=
	Ge,
	/// >
	Gt,
}

impl From<&str> for PkgQCond {
	fn from(value: &str) -> Self {
		match value {
			"=" => Self::Eq,
			">=" => Self::Ge,
			">" => Self::Gt,
			"<=" => Self::Le,
			"<" => Self::Lt,
			_ => unreachable!("Regex RE_PKGQCOND matched bad condition `{value}`"),
		}
	}
}

/// Denotes a package dependency.
///
/// This is used to represent a package specified in `Requires:` or `BuildRequires:`.
///
/// # Examples
/// ```
/// let pkg = Package::new("anda");
///
/// let recommends = vec![];
/// Package::add_simple_query(&mut recommends, "subatomic terra-release, mock")?;
/// # Ok::<(), color_eyre::Report>(())
/// ```
#[derive(Clone, Default, Debug)]
pub struct Package {
	/// Name of the package dependency
	pub name: String,
	/// Version (right hand side of the dependency query)
	pub version: Option<String>,
	/// Release (right hand side of the dependency query)
	pub release: Option<String>,
	/// Epoch (right hand side of the dependency query)
	pub epoch: Option<u32>,
	/// Conditional operator (middle of the dependency query)
	pub condition: PkgQCond,
}

impl Package {
	/// Creates a new Dependency representation with the package name.
	#[must_use]
	pub fn new(name: String) -> Self {
		Self { name, ..Self::default() }
	}
	/// Parses a simple query, i.e. packages specified without conditionals and versions.
	///
	/// The names of the packages should be separated either by spaces or commas, just like in
	/// `Recommends:`.
	///
	/// # Examples
	/// ```
	/// let mut pkgs = vec![];
	/// Package::add_simple_query(&mut pkgs, "anda subatomic, mock")?;
	/// # Ok::<(), color_eyre::Report>(())
	/// ```
	///
	/// # Errors
	/// An error is returned if and only if there exists an invalid character that is
	/// not alphanumeric, a space, a comma, a dash and an underscore.
	pub fn add_simple_query(pkgs: &mut Vec<Self>, query: &str) -> Result<()> {
		let query = query.trim();
		let mut last = String::new();
		for ch in query.chars() {
			if (ch == ' ' || ch == ',') && !last.is_empty() {
				pkgs.push(Self::new(std::mem::take(&mut last)));
				continue;
			}
			if ch.is_alphanumeric() || PKGNAMECHARSET.contains(ch) {
				return Err(eyre!("Invalid character `{ch}` found in package query.").note(format!("query: `{query}`")));
			}
			last.push(ch);
		}
		if !last.is_empty() {
			pkgs.push(Self::new(last));
		}
		Ok(())
	}
	/// Parses a query.
	///
	/// Each package query that may contains a [condition](PkgQCond) and a version ('Dependency')
	/// should be separated by spaces or commas. There should also be spaces around the
	/// conditional operators, including `=`.
	///
	/// # Errors
	/// An error is returned if and only if
	/// - there exists an invalid character in package names that is not alphanumeric, a space, a
	///   comma, a dash and an underscore; or
	/// - an epoch specified cannot be parsed by `core::str::parse::<u32>()`.
	///
	/// # Panics
	/// - [`Regex`] is used to parse the conditions.
	///   A panic might occurs if a capture group is not found via `caps[n]`.
	///   However, This is unlikely since the groups either exist in the regex
	///   or the optional group is accessed using `caps.get(n)`.
	/// - A panic might occurs if it fails to strip the `:` suffix from the regex capture group for
	///   the epoch, but again this is unlikely.
	///
	/// # Caveats
	/// This function is recursive, but it should be safe.
	pub fn add_query(pkgs: &mut Vec<Self>, query: &str) -> Result<()> {
		let query = query.trim(); // just in case
		if query.is_empty() {
			return Ok(());
		}
		let Some((name, rest)) = query.split_once(|c: char| !c.is_alphanumeric() && !PKGNAMECHARSET.contains(c)) else {
			// check if query matches pkg name
			if query.chars().any(|c| !c.is_alphanumeric() && !PKGNAMECHARSET.contains(c)) {
				return Err(eyre!("Invalid package name `{query}`").suggestion("Use only alphanumerics, underscores and dashes."));
			}
			pkgs.push(Self::new(query.into()));
			return Ok(())
		};
		// the part that matches the good name is `name`. Check the rest.
		let mut pkg = Self::new(name.into());
		let Some(caps) = RE_PKGQCOND.captures(rest) else {
			return Self::add_query(pkgs, rest.trim_start_matches(|c| " ,".contains(c)));
		};
		pkg.condition = caps[1].into();
		if let Some(epoch) = caps.get(2) {
			let epoch = epoch.as_str().strip_suffix(':').expect("epoch no `:` by RE_PKGQCOND");
			pkg.epoch = Some(epoch.parse().map_err(|e| eyre!("Cannot parse epoch to u32: `{epoch}`").error(e).suggestion("Epoch can only be positive integers"))?);
		}
		pkg.version = Some(caps[3].into());
		pkg.release = Some(caps[4].into());
		pkgs.push(pkg);
		if let Some(rest) = caps.get(5) {
			return Self::add_query(pkgs, rest.as_str().trim_start_matches(|c| " ,".contains(c)));
		}
		Ok(())
	}
}

/// Represents the `Requires:` preamble.
///
/// Each attribute/property in [`Self`] represents the `Requires(...):` syntax.
#[derive(Default, Clone, Debug)]
pub struct RPMRequires {
	/// Dependencies listed in `Requires:`.
	pub none: Vec<Package>,
	/// Dependencies listed in `Requires(pre):`.
	pub pre: Vec<Package>,
	/// Dependencies listed in `Requires(post):`.
	pub post: Vec<Package>,
	/// Dependencies listed in `Requires(preun):`.
	pub preun: Vec<Package>,
	/// Dependencies listed in `Requires(postun):`.
	pub postun: Vec<Package>,
	/// Dependencies listed in `Requires(pretrans):`.
	pub pretrans: Vec<Package>,
	/// Dependencies listed in `Requires(posttrans):`.
	pub posttrans: Vec<Package>,
	/// Dependencies listed in `Requires(verify):`.
	pub verify: Vec<Package>,
	/// Dependencies listed in `Requires(interp):`.
	pub interp: Vec<Package>,
	/// Dependencies listed in `Requires(meta):`.
	pub meta: Vec<Package>,
}

// todo https://docs.fedoraproject.org/en-US/packaging-guidelines/Scriptlets/
/// Scriptlets like `%pre`, `%posttrans`, etc.
#[derive(Default, Clone, Debug)]
pub struct Scriptlets {
	/// `%pre` scriptlet.
	pub pre: Option<String>,
	/// `%post` scriptlet.
	pub post: Option<String>,
	/// `%preun` scriplets.
	pub preun: Option<String>,
	/// `%postun` scriplets.
	pub postun: Option<String>,
	/// `%pretrans` scriplets.
	pub pretrans: Option<String>,
	/// `%posttrans` scriplets.
	pub posttrans: Option<String>,
	/// `%verify` scriplets.
	pub verify: Option<String>,

	/// `%triggerprein` scriptlets.
	pub triggerprein: Option<String>,
	/// `%triggerin` scriptlets.
	pub triggerin: Option<String>,
	/// `%triggerun` scriptlets.
	pub triggerun: Option<String>,
	/// `%triggerpostun` scriptlets.
	pub triggerpostun: Option<String>,

	/// `%filetriggerin` scriptlets.
	pub filetriggerin: Option<String>,
	/// `%filetriggerun` scriptlets.
	pub filetriggerun: Option<String>,
	/// `%filetriggerpostun` scriptlets.
	pub filetriggerpostun: Option<String>,
	/// `%transfiletriggerin` scriptlets.
	pub transfiletriggerin: Option<String>,
	/// `%transfiletriggerun` scriptlets.
	pub transfiletriggerun: Option<String>,
	/// `%transfiletriggerpostun` scriptlets.
	pub transfiletriggerpostun: Option<String>,
}

/// Settings for `%config(...)` in `%files`.
///
/// - [`ConfigFileMod::MissingOK`] : `%config(missingok)`
/// - [`ConfigFileMod::NoReplace`] : `%config(noreplace)`
/// - [`ConfigFileMod::None`]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum ConfigFileMod {
	/// Represents `%config`
	#[default]
	None,
	/// Represents `%config(missingok)`
	MissingOK,
	/// Represents `%config(noreplace)`
	NoReplace,
}

/// Settings for `%verify(...)` in `%files`.
///
/// - [`VerifyFileMod::Owner`] : `%verify(user owner)`
/// - [`VerifyFileMod::Group`] : `%verify(group)`
/// - [`VerifyFileMod::Mode`] : `%verify(mode)`
/// - [`VerifyFileMod::Md5`] : `%verify(filedigest md5)`
/// - [`VerifyFileMod::Size`] : `%verify(size)`
/// - [`VerifyFileMod::Maj`] : `%verify(maj)`
/// - [`VerifyFileMod::Min`] : `%verify(min)`
/// - [`VerifyFileMod::Symlink`] : `%verify(link symlink)`
/// - [`VerifyFileMod::Rdev`] : `%verify(rdev)`
/// - [`VerifyFileMod::Mtime`] : `%verify(mtime)`
/// - [`VerifyFileMod::Not`] :`%verify(not ...)`
///
/// For [`VerifyFileMod::None(String)`], the `String` is the input into `VerifyFileMod::from()`.
/// This means the input is not recognised as a valid `%verify()` setting.
///
/// # See also
/// - <https://rpm-software-management.github.io/rpm/manual/spec.html#virtual-file-attributes>
/// - <http://ftp.rpm.org/max-rpm/s1-rpm-inside-files-list-directives.html>
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyFileMod {
	/// Represents `%verify(user owner)`
	Owner,
	/// Represents `%verify(group)`
	Group,
	/// Represents `%verify(mode)`
	Mode,
	/// Represents `%verify(filedigest md5)`
	Md5,
	/// Represents `%verify(size)`
	Size,
	/// Represents `%verify(maj)`
	Maj,
	/// Represents `%verify(min)`
	Min,
	/// Represents `%verify(link symlink)`
	Symlink,
	/// Represents `%verify(mtime)`
	Mtime,
	/// Represents `%verify(rdev)`
	Rdev,
	/// Represents `%verify(...)` where `...` is invalid
	None(String),
	/// Represents `%verify(not ...)`
	Not,
}

impl VerifyFileMod {
	/// Returns all the possible arguments to `%verify`
	#[must_use]
	pub fn all() -> Box<[Self]> {
		use VerifyFileMod::{Group, Maj, Md5, Min, Mode, Mtime, Owner, Size, Symlink};
		vec![Owner, Group, Mode, Md5, Size, Maj, Min, Symlink, Mtime].into_boxed_slice()
	}
}

impl From<&str> for VerifyFileMod {
	fn from(value: &str) -> Self {
		use VerifyFileMod::{Group, Maj, Md5, Min, Mode, Mtime, None, Not, Owner, Rdev, Size, Symlink};
		match value {
			"user" | "owner" => Owner,
			"group" => Group,
			"mode" => Mode,
			"filedigest" | "md5" => Md5,
			"size" => Size,
			"maj" => Maj,
			"min" => Min,
			"link" | "symlink" => Symlink,
			"rdev" => Rdev,
			"mtime" => Mtime,
			"not" => Not,
			_ => None(value.into()),
		}
	}
}

/// File derivatives used in `%files`.
///
/// - `RPMFileAttr::Artifact`
/// - `RPMFileAttr::Ghost`
/// - `RPMFileAttr::Config(ConfigFileMod)` (See [`ConfigFileMod`])
/// - `RPMFileAttr::Dir`
/// - `RPMFileAttr::Doc`
/// - `RPMFileAttr::License`
/// - `RPMFileAttr::Verify(Box<[VerifyFileMod]>)` (See [`VerifyFileMod`])
/// - `RPMFileAttr::Docdir`
/// - `RPMFileAttr::Normal` (files without derivatives use this)
///
/// # See also
/// - <https://rpm-software-management.github.io/rpm/manual/spec.html#virtual-file-attributes>
/// - <http://ftp.rpm.org/max-rpm/s1-rpm-inside-files-list-directives.html>
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub enum RPMFileAttr {
	/// Represents `%artifact`
	Artifact,
	/// Represents `%ghost`
	Ghost,
	/// Represents `%config(...)`, see [`ConfigFileMod`]
	Config(ConfigFileMod),
	/// Represents `%dir`
	Dir,
	/// Represents `%doc`
	Doc,
	/// Represents `%license`
	License,
	/// Represents `%verify(...)`, see [`VerifyFileMod`]
	Verify(Box<[VerifyFileMod]>),
	/// Represents `%docdir`
	Docdir,
	/// Represents files without file derivatives
	#[default]
	Normal,
}

/// Represents a file in `%files`.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RPMFile {
	/// File derivative
	pub attr: RPMFileAttr,

	/// the file / dir path
	pub path: String,
	/// permission / mode
	pub mode: u16,
	/// user / owner that owns the file
	pub user: String,
	/// group that owns the file
	pub group: String,
	/// directory permission / mode
	pub dmode: u16,
}

/// Represents a `%files` section.
#[derive(Default, Clone, Debug)]
pub struct RPMFiles {
	/// Represents `%files -f ...`, the argument for `-f` is the file with a list of files to
	/// include in the `%files` section. It is NOT processed here; you should use an RPM builder.
	pub incl: String,
	/// Files listed in `%files`
	pub files: Box<[RPMFile]>,
	/// The raw `%files` sections
	pub raw: String,
}

impl RPMFiles {
	/// Parses a `%files` section using `self.raw`.
	fn parse(&mut self) -> Result<()> {
		//? http://ftp.rpm.org/max-rpm/s1-rpm-inside-files-list-directives.html
		let mut defattr = (0, "".into(), "".into(), 0);
		self.files = RE_FILE
			.captures_iter(&self.raw)
			.map(|cap| {
				if let Some(remain) = &cap[0].strip_prefix("%defattr(") {
					let Some(remain) = remain.trim_end().strip_suffix(')') else { return Err(eyre!("Closing `)` not found for `%defattr(`")) };
					let ss: Box<[&str]> = remain.split(',').map(str::trim).collect();
					let [filemode, user, group, dirmode] = match *ss {
						[filemode, user, group] => [filemode, user, group, ""],
						[filemode, user, group, dmode] => [filemode, user, group, dmode],
						_ => return Err(eyre!("Expected 3/4 arguments for %defattr(), found {}", ss.len())),
					};
					defattr = (
						if filemode == "-" { 0 } else { filemode.parse().map_err(|e: ParseIntError| eyre!(e).wrap_err("Cannot parse file mode"))? },
						(if user == "-" { "" } else { user }).into(),
						(if group == "-" { "" } else { group }).into(),
						if dirmode == "-" { 0 } else { dirmode.parse().map_err(|e: ParseIntError| eyre!(e).wrap_err("Cannot parse dir mode"))? },
					);
					return Ok(RPMFile::default());
				}
				let mut f = RPMFile::default();
				if let Some(name) = cap.get(1) {
					let name = name.as_str();
					if let Some(m) = cap.get(2) {
						let x = m.as_str().strip_prefix('(').expect("RE_FILE not matching parens `(...)` but found capture group 2");
						let x = x.strip_suffix(')').expect("RE_FILE not matching parens `(...)` but found capture group 2");
						if name.starts_with("%attr(") {
							let ss: Vec<&str> = x.split(',').map(str::trim).collect();
							let Some([fmode, user, group]) = ss.get(0..=2) else {
								return Err(eyre!("Expected 3 arguments in `%attr(...)`"));
							};
							let (fmode, user, group) = (*fmode, *user, *group);
							if fmode != "-" {
								f.mode = fmode.parse().map_err(|e: ParseIntError| eyre!(e).wrap_err("Cannot parse file mode"))?;
							}
							if user != "-" {
								f.user = user.into();
							}
							if group != "-" {
								f.group = group.into();
							}
							f.path = cap.get(3).expect("No RE grp 3 in %files?").as_str().into();
							return Ok(f);
						}
						if name.starts_with("%verify(") {
							let mut vs: Vec<_> = x.split(' ').map(VerifyFileMod::from).collect();
							for v in &vs {
								if let VerifyFileMod::None(s) = v {
									return Err(eyre!("`%verify({s})` is unknown"));
								}
							}
							if vs.contains(&VerifyFileMod::Not) {
								let mut ll = VerifyFileMod::all().to_vec();
								ll.retain(|x| !vs.contains(x));
								vs = ll;
							}
							f.attr = RPMFileAttr::Verify(vs.into_boxed_slice());
							f.path = cap.get(3).expect("No RE grp 3 in %files?").as_str().into();
							(f.mode, f.user, f.group, f.dmode) = defattr.clone();

							return Ok(f);
						}
						if name.starts_with("%config(") {
							f.attr = RPMFileAttr::Config(match x {
								"missingok" => ConfigFileMod::MissingOK,
								"noreplace" => ConfigFileMod::NoReplace,
								_ => return Err(eyre!("`%config({x})` is unknown")),
							});
							f.path = cap.get(3).expect("No RE grp 3 in %files?").as_str().into();
							(f.mode, f.user, f.group, f.dmode) = defattr.clone();
						}
						return Err(eyre!("Unknown %files directive: %{name}"));
					}
					f.attr = match name {
						"%artifact " => RPMFileAttr::Artifact,
						"%ghost " => RPMFileAttr::Ghost,
						"%config " => RPMFileAttr::Config(ConfigFileMod::None),
						"%dir " => RPMFileAttr::Dir,
						"%doc " | "%readme " => RPMFileAttr::Doc,
						"%license " => RPMFileAttr::License,
						"%docdir " => RPMFileAttr::Docdir,
						_ => return Err(eyre!("Unknown %files directive: %{name}")),
					}
				}
				f.path = cap.get(3).expect("No RE grp 3 in %files?").as_str().into();
				Ok(f)
			})
			.filter(|x| x.as_ref().map_or(false, |x| x.path.is_empty()))
			.collect::<Result<Box<[RPMFile]>>>()?;
		Ok(())
	}
}

/// Represents 1 changelog entry in `%changelog`.
///
/// # Example
/// ```
/// let mut changelog = Changelog {
///   date: NaiveDate::from_ymd_opt(2006, 1, 1)?,
///   version: Some("1.11.0-6"),
///   maintainer: "madomado",
///   email: Some("madonuko@outlook.com"),
///   message: "- messages here\n- *markdown magic* here\n- version and email is optional",
/// };
/// ```
#[derive(Default, Clone, Debug)]
pub struct Changelog {
	/// Date of changelog
	pub date: chrono::NaiveDate,
	/// Version corresponding to the changelog entry
	pub version: Option<String>,
	/// The person who created the changelog
	pub maintainer: String,
	/// Email of the maintainer
	pub email: Option<String>,
	/// Message of the changelog
	pub message: String,
}

/// Represents a `%changelog` section.
///
/// # Example
/// Let's look at this changelog:
/// ```rpmspec
/// * Wed Jan 11 2006 madomado <madonuko@outlook.com> - 1.11.0-6
/// - messages here
/// - *markdown magic* here
/// - version and email is optional
/// ```
/// in rust:
/// ```
/// let mut cl = rpmspec_rs::parse::Changelogs::default();
/// cl.raw = r#"
/// * Wed Jan 11 2006 madomado <madonuko@outlook.com> - 1.11.0-6
/// - messages here
/// - *markdown magic* here
/// - version and email is optional
/// "#.into();
/// cl.parse()?;
/// // everything is now in `cl.changelogs`!
/// # Ok::<(), color_eyre::Report>(())
/// ```
#[derive(Default, Clone, Debug)]
pub struct Changelogs {
	/// an immutable boxed vector of [`Changelog`]
	pub changelogs: Box<[Changelog]>,
	/// changelogs that are not (yet) parsed
	pub raw: String,
}

impl Changelogs {
	/// Parses a `%changelog` section.
	///
	/// # Example
	/// Let's look at this changelog:
	/// ```rpmspec
	/// * Wed Jan 11 2006 madomado <madonuko@outlook.com> - 1.11.0-6
	/// - messages here
	/// - *markdown magic* here
	/// - version and email is optional
	/// ```
	/// in rust:
	/// ```
	/// let mut cl = rpmspec_rs::parse::Changelogs::default();
	/// cl.raw = r#"
	/// * Wed Jan 11 2006 madomado <madonuko@outlook.com> - 1.11.0-6
	/// - messages here
	/// - *markdown magic* here
	/// - version and email is optional
	/// "#.into();
	/// cl.parse()?;
	/// // everything is now in `cl.changelogs`!
	/// # Ok::<(), color_eyre::Report>(())
	/// ```
	///
	/// # Errors
	/// - [`chrono::ParseError`] if any dates cannot be parsed.
	pub fn parse(&mut self) -> Result<()> {
		self.changelogs = RE_CHANGELOG
			.captures_iter(&self.raw)
			.map(|cap| {
				Ok(Changelog {
					date: chrono::NaiveDate::parse_from_str(&cap[1], "%a %b %d %Y").map_err(|e| eyre!(e).wrap_err("Cannot parse date in %changelog"))?,
					version: cap.get(10).map(|v| v.as_str().into()),
					maintainer: cap[6].into(),
					email: cap.get(8).map(|email| email.as_str().into()),
					message: cap[11].trim().into(),
				})
			})
			.collect::<Result<Box<[Changelog]>>>()?;
		Ok(())
	}
}

/// Represents different sections in an RPM spec file.
#[derive(Default, Clone, Debug)]
pub enum RPMSection {
	/// The global preamble section.
	#[default]
	Global,
	/// A subpackage (`%package ...`)
	Package(String),
	/// Description (`%description [...]`)
	Description(String),
	/// %prep
	Prep,
	/// %build
	Build,
	/// %install
	Install,
	/// %files [...] [-f ...]
	Files(String, Option<String>),
	/// %changelog
	Changelog,
}

/// Represents a subpackage (`%package ...`).
#[derive(Default, Clone, Debug)]
pub struct RPMSpecPkg {
	/// Name of subpackage (`%package [-n] ...`)
	///
	/// If `-n` is used, the argument following `-n` is the name.
	/// Otherwise, the prefix `%{name}-` will be added.
	pub name: Option<String>,
	/// Summary of subpackage (`Summary:`)
	pub summary: String,
	/// Dependencies of subpackage listed in `Requires:`
	pub requires: RPMRequires,
	/// Description of subpackage (`%description [-n] ...`)
	pub description: String,
	/// Group of subpackage (`Group:`)
	pub group: Option<String>,
	/// What the subpackage `Provides:`
	pub provides: Vec<Package>,
	/// Represents `Conflicts:`
	pub conflicts: Vec<Package>,
	/// Represents `Obsoletes:`
	pub obsoletes: Vec<Package>,
	/// Represents `Recommends:`
	pub recommends: Vec<Package>,
	/// Represents `Suggests:`
	pub suggests: Vec<Package>,
	/// Represents `Supplements:`
	pub supplements: Vec<Package>,
	/// Represents `Enhances:`
	pub enhances: Vec<Package>,
	/// Files in subpackage listed in `%files [-n] ...`
	pub files: RPMFiles,
	/// Scriptlets present in the final RPM package, such as `%post [-n] ...` and `%pretrans [-n] ...`
	pub scriptlets: Scriptlets, // todo
}

/// Represents the entire spec file.
#[derive(Default, Clone, Debug)]
pub struct RPMSpec {
	/// List of subpackages (`%package ...`).
	pub packages: HashMap<String, RPMSpecPkg>,

	/// Represents `%description`
	pub description: String,
	/// Represents `%prep`
	pub prep: String,
	/// Represents `%generate_buildrequires`
	pub generate_buildrequires: Option<String>,
	/// Represents `%conf`
	pub conf: Option<String>,
	/// Represents `%build`
	pub build: String,
	/// Represents `%install`
	pub install: String,
	/// Represents `%check`
	pub check: String,

	/// Scriptlets present in the final RPM package, such as `%post` and `%pretrans`
	pub scriptlets: Scriptlets,
	/// Files present in the final RPM package listed in `%files [-f ...]`
	pub files: RPMFiles,
	/// Represents `%changelog`
	pub changelog: Changelogs,

	/// Represents `Name:`
	pub name: Option<String>,
	/// Represents `Version:`
	pub version: Option<String>,
	/// Represents `Release:`
	pub release: Option<String>,
	/// Represents `Epoch:`
	pub epoch: Option<i32>,
	/// Repreesnts `License:`
	pub license: Option<String>,
	/// Repreesnts `SourceLicense:`
	pub sourcelicense: Option<String>,
	/// Repreesnts `Group:`
	pub group: Option<String>,
	/// Repreesnts `Summary:`
	pub summary: Option<String>,
	/// Repreesnts `Source0:`, `Source1:`, ...
	pub sources: HashMap<u32, String>,
	/// Repreesnts `Patch0:`, `Patch1:`, ...
	pub patches: HashMap<u32, String>,
	// TODO icon
	// TODO nosource nopatch
	/// Represents `URL:`
	pub url: Option<String>,
	/// Represents `BugURL:`
	pub bugurl: Option<String>,
	/// Represents `ModularityLabel:`
	pub modularitylabel: Option<String>,
	/// Represents `DistTag:`
	pub disttag: Option<String>,
	/// Represents `VCS:`
	pub vcs: Option<String>,
	/// Represents `Distribution:`
	pub distribution: Option<String>,
	/// Represents `Vendor:`
	pub vendor: Option<String>,
	/// Represents `Packager:`
	pub packager: Option<String>,
	// TODO buildroot
	/// Represents `AutoReqProv:`
	pub autoreqprov: bool,
	/// Represents `AutoReq:`
	pub autoreq: bool,
	/// Represents `AutoProv:`
	pub autoprov: bool,
	/// Represents `Requires:` and `Requires(...):`
	pub requires: RPMRequires,
	/// Represents `Provides:`
	pub provides: Vec<Package>,
	/// Represents `Conflicts:`
	pub conflicts: Vec<Package>,
	/// Represents `Obsoletes:`
	pub obsoletes: Vec<Package>,
	/// Represents `Recommends:`
	pub recommends: Vec<Package>,
	/// Represents `Suggests:`
	pub suggests: Vec<Package>,
	/// Represents `Supplements:`
	pub supplements: Vec<Package>,
	/// Represents `Enhances:`
	pub enhances: Vec<Package>,
	/// Represents `OrderWithRequires:`
	pub orderwithrequires: Vec<Package>,
	/// Represents `BuildRequires:`
	pub buildrequires: Vec<Package>,
	/// Represents `BuildConflicts:`
	pub buildconflicts: Vec<Package>,
	/// Represents `ExcludeArch:`
	pub excludearch: Vec<String>,
	/// Represents `ExclusiveArch:`
	pub exclusivearch: Vec<String>,
	/// Represents `ExcludeOS:`
	pub excludeos: Vec<String>,
	/// Represents `ExclusiveOS:`
	pub exclusiveos: Vec<String>,
	/// Represents `BuildArch:`, `BuildArchitectures:`
	pub buildarch: Vec<String>,
	/// Represents `Prefix:`, `Prefixes:`
	pub prefix: Option<String>,
	/// Represents `Docdir:`
	pub docdir: Option<String>,
	/// Represents `RemovePathPostFixes:`
	pub removepathpostfixes: Vec<String>,
}

impl RPMSpec {
	/// Creates a new RPM spec object with good defaults:
	/// - `autoreqprov: true`
	/// - `autoreq: true`
	/// - `autoprov: true`
	///
	/// # Examples
	///
	/// ```
	/// use rpmspec_rs::parse::RPMSpec;
	///
	/// assert_eq!(RPMSpec::new(), RPMSpec {
	///   autoreqprov: true,
	///   autoreq: true,
	///   autoprov: true,
	///   ..Self::default()
	/// });
	/// ```
	#[must_use]
	pub fn new() -> Self {
		Self {
			// buildroot
			autoreqprov: true,
			autoreq: true,
			autoprov: true,
			..Self::default()
		}
	}
}

/// An RPM spec parser.
///
/// # Examples
/// ```
/// let mut parser = rpmspec_rs::parse::SpecParser::new();
/// parser.parse(std::io::BufReader::new(b"%define hai bai\nName: %hai" as &[u8]))?;
/// assert_eq!(parser.rpm.name, Some("bai".into()));
/// # Ok::<(), color_eyre::Report>(())
/// ```
#[derive(Default, Clone, Debug)]
pub struct SpecParser {
	/// The parsed RPM package
	pub rpm: RPMSpec,
	errors: Vec<ParserError>,
	/// Macros present in the spec file union the system macros
	pub macros: HashMap<String, String>,
	section: RPMSection,
	cond: Vec<(bool, bool)>, // current, before

	pub(crate) count_line: usize,
	pub(crate) count_col: usize,
	pub(crate) count_chard: usize,
}

impl SpecParser {
	/// Returns an iterator that yields characters such that the macros in the input are parsed
	pub fn parse_macro<'a>(&'a mut self, reader: &'a mut Consumer) -> SpecMacroParserIter {
		SpecMacroParserIter { reader, parser: self, percent: false, buf: String::new() }
	}

	/// Parse the `Requires:` or `Requires(...):` preambles.
	///
	/// # Errors
	/// - only if the dependency specified is invalid ([`Package::add_query`])
	pub fn parse_requires(&mut self, sline: &str) -> Result<bool> {
		let Some(caps) = RE_REQ1.captures(sline) else {
			return Ok(false);
		};
		let mut pkgs = vec![];
		Package::add_query(&mut pkgs, caps[2].trim())?;
		let modifiers = if caps.len() == 2 { &caps[2] } else { "none" };
		for modifier in modifiers.split(',') {
			let modifier = modifier.trim();
			let pkgs = pkgs.clone();
			let r = if let RPMSection::Package(ref p) = self.section { &mut self.rpm.packages.get_mut(p).expect("No subpackage when parsing Requires").requires } else { &mut self.rpm.requires };
			match modifier {
				"none" => r.none.extend(pkgs),
				"pre" => r.pre.extend(pkgs),
				"post" => r.post.extend(pkgs),
				"preun" => r.preun.extend(pkgs),
				"postun" => r.postun.extend(pkgs),
				"pretrans" => r.pretrans.extend(pkgs),
				"posttrans" => r.posttrans.extend(pkgs),
				"verify" => r.verify.extend(pkgs),
				"interp" => r.interp.extend(pkgs),
				"meta" => r.meta.extend(pkgs),
				_ => self.errors.push(ParserError::UnknownModifier(self.count_line, modifier.into())),
			}
		}
		Ok(true)
	}

	/// Returns the architecture of the system using `uname -m`
	///
	/// # Errors
	/// - [`std::io::Error`] if command fails to execute
	/// - [`std::io::Utf8Error`] if command output cannot be parsed
	pub fn arch() -> Result<String> {
		let binding = Command::new("uname").arg("-m").output()?;
		let s = core::str::from_utf8(&binding.stdout)?;
		Ok(s[..s.len() - 1].into()) // remove new line
	}

	// todo rewrite
	/// Loads all macros defined in a file.
	///
	/// # Errors
	/// - [`io::Error`] when it fails open/read the file
	/// - [`core::str::Utf8Error`] when the file content cannot be converted into `&str`
	///
	/// # Panics
	/// - Cannot unwrap static [`Regex`] (0% chance of happening)
	pub fn load_macro_from_file(&mut self, path: &std::path::Path) -> Result<()> {
		lazy_static::lazy_static! {
			static ref RE: Regex = Regex::new(r"(?m)^%([\w()]+)[\t ]+((\\\n|[^\n])+)$").unwrap();
		}
		debug!("Loading macros from {}", path.display());
		let mut buf = vec![];
		let bytes = BufReader::new(std::fs::File::open(path)?).read_to_end(&mut buf)?;
		debug_assert_ne!(bytes, 0, "Empty macro definition file '{}'", path.display());
		for cap in RE.captures_iter(std::str::from_utf8(&buf)?) {
			if let Some(val) = self.macros.get(&cap[1]) {
				debug!("Macro Definition duplicated: {} : '{val:?}' | '{}'", &cap[1], &cap[2]);
				continue; // FIXME?
			}
			let name = &cap[1];
			if let Some(name) = name.strip_suffix("()") {
				self.macros.insert(name.into(), format!("{} ", &cap[2]).into());
			} else {
				// we trim() just in case
				self.macros.insert(cap[1].into(), cap[2].trim().into());
			}
		}
		Ok(())
	}

	/// Loads all system macros via the `Macro path` entry in `rpm --showrc`.
	///
	/// # Errors
	/// - [`io::Error`] when `sh -c "rpm --showrc | grep '^Macro path' | sed 's/Macro path: //'"` fails to run
	/// - [`core::str::Utf8Error`] when the output of the above command cannot be parsed into `&str`
	/// - [`io::Error`] and [`core::str::Utf8Error`] from `uname -m` ([`SpecParser::arch()`])
	/// - [`glob::PatternError`] if the macro paths from the `rpm` command output are invalid
	/// - [`io::Error`] when [`SpecParser::load_macro_from_file()`] fails to open/read the file
	/// - [`core::str::Utf8Error`] when the file content cannot be converted into `&str`
	///
	/// # Caveats
	/// Not sure where I've seen the docs, but there was one lying around saying you can define multiple
	/// macros with the same name, and when you undefine it the old one recovers (stack?). I don't think
	/// it is a good idea to do it like that (it is simply ridiculous and inefficient) but you can try.
	pub fn load_macros(&mut self) -> Result<()> {
		// run rpm --showrc | grep "^Macro path"
		let binding = Command::new("sh").args(["-c", "rpm --showrc|grep '^Macro path'|sed 's/Macro path: //'"]).output()?;
		let binding = core::str::from_utf8(&binding.stdout)?;
		let paths = binding.trim().split(':');

		// TODO use Consumer::read_til_EOL() instead
		for path in paths {
			let path = path.replace("%{_target}", Self::arch()?.as_str());
			debug!(": {path}");
			for path in glob::glob(path.as_str())? {
				self.load_macro_from_file(&path?)?;
			}
		}
		Ok(())
	}

	/// Handles conditions as if they are sections, like `%if` and `%elifarch`, etc.
	///
	/// # Errors
	/// - [`std::io::Error`] or [`std::io::Utf8Error`] when cannot detect arch via [`SpecParser::arch()`]
	pub fn _handle_conditions(&mut self, start: &str, remain: &str) -> Result<bool> {
		match start {
			"if" => {
				let c = remain.parse().map_or(true, |n: isize| n != 0);
				self.cond.push((c, c));
			}
			"ifarch" => {
				let c = remain == Self::arch()?;
				self.cond.push((c, c));
			}
			"ifnarch" => {
				let c = remain != Self::arch()?;
				self.cond.push((c, c));
			}
			"elifarch" => {
				let Some((a, b)) = self.cond.last_mut() else { return Err(eyre!("%elifarch found without %if/%ifarch"))};
				if *b {
					*a = false;
				} else {
					*a = remain == Self::arch()?;
					*b = *a;
				}
			}
			"elifnarch" => {
				let Some((a, b)) = self.cond.last_mut() else { return Err(eyre!("%elifarch found without %if/%ifarch"))};
				if *b {
					*a = false;
				} else {
					*a = remain != Self::arch()?;
					*b = *a;
				}
			}
			"elif" => {
				let Some((a, b)) = self.cond.last_mut() else {return Err(eyre!("%elif found without %if"))};
				if *b {
					*a = false;
				} else {
					*a = remain.parse().map_or(true, |n: isize| n != 0);
					*b = *a;
				}
			}
			"else" => {
				let Some((a, b)) = self.cond.last_mut() else {return Err(eyre!("%else found without %if"))};
				if *b {
					*a = false;
				} else {
					*a = !(*a);
					// *b = *a; (doesn't matter)
				}
			}
			"endif" => return if self.cond.pop().is_none() { Err(eyre!("%endif found without %if")) } else { Ok(true) },
			_ => return Ok(false),
		}
		Ok(true)
	}

	/// Detect a section in a spec file and returns `Ok(true)` if the line is processed.
	///
	/// # Errors
	/// - Invalid syntax. See the error message. (of type [`color_eyre::Report`])
	/// - Fail to get arch ([`Self::arch()`]) via `uname -m`
	pub fn _handle_section(&mut self, l: &str) -> Result<bool> {
		if l.contains('\n') {
			return Ok(false);
		}
		if let Some((false, _)) = self.cond.last() {
			return Ok(true); // false condition, skip parsing
		}
		let Some((start, remain)) = l.split_once(' ') else {
			return Ok(false);
		};
		let remain = remain.trim();
		if !(start.starts_with('%') && start.chars().nth(1) != Some('%')) {
			return Ok(false);
		}
		if self._handle_conditions(&start[1..], remain)? {
			return Ok(true);
		}
		self.section = match &start[1..] {
			"description" if remain.is_empty() => RPMSection::Description("".into()),
			"description" => RPMSection::Description({
				let (_, mut args, flags) = self._param_macro_line_args(&mut remain.into()).map_err(|e| e.wrap_err("Cannot parse arguments to %description"))?;
				if let Some(x) = flags.iter().find(|x| **x != 'n') {
					return Err(eyre!("Unexpected %description flag `-{x}`"));
				}
				if args.len() != 1 {
					return Err(eyre!("Expected 1, found {} arguments (excluding flags) to %description", args.len()));
				}
				let arg = unsafe { args.get_unchecked_mut(0) };
				if flags.is_empty() {
					format!("{}-{arg}", self.rpm.name.as_ref().ok_or(eyre!("Expected package name before subpackage `{arg}`"))?).into()
				} else {
					take(arg)
				}
			}),
			"package" if remain.is_empty() => return Err(eyre!("Expected arguments to %package")),
			"package" => {
				let (_, mut args, flags) = self._param_macro_line_args(&mut remain.into()).map_err(|e| e.wrap_err("Cannot parse arguments to %package"))?;
				if let Some(x) = flags.iter().find(|x| **x != 'n') {
					return Err(eyre!("Unexpected %package flag `-{x}`"));
				}
				if args.len() != 1 {
					return Err(eyre!("Expected 1, found {} arguments (excluding flags) to %package", args.len()));
				}
				let arg = unsafe { args.get_unchecked_mut(0) };
				let name = if flags.is_empty() { format!("{}-{arg}", self.rpm.name.as_ref().ok_or(eyre!("Expected package name before subpackage `{arg}`"))?).into() } else { take(arg) };
				if self.rpm.packages.contains_key(&name) {
					return Err(eyre!("The subpackage {name} has already been declared"));
				}
				self.rpm.packages.insert(name.clone(), RPMSpecPkg::default());
				RPMSection::Package(name)
			}
			"prep" => RPMSection::Prep,
			"build" => RPMSection::Build,
			"install" => RPMSection::Install,
			"files" => {
				let mut f = None;
				let mut name: String = "".into();
				let mut remains = remain.split(' ');
				while let Some(remain) = remains.next() {
					if let Some(flag) = remain.strip_prefix('-') {
						match flag {
							"f" => {
								let Some(next) = remains.next() else {
									return Err(eyre!("Expected argument for %files after `-f`"));
								};
								if next.starts_with('-') {
									return Err(eyre!("Expected argument for %files after `-f`, found flag `{next}`"));
								}
								if let Some(old) = f {
									return Err(eyre!("Unexpected duplicated `-f`").note(format!("Old: {old}")).note(format!("New: {next}")));
								}
								f = Some(next.into());
							}
							"n" => {
								let Some(next) = remains.next() else {
									return Err(eyre!("Expected argument for %files after `-n`"));
								};
								if next.starts_with('-') {
									return Err(eyre!("Expected argument for %files after `-n`, found flag `{next}`"));
								}
								if !name.is_empty() {
									return Err(eyre!("The name of the subpackage is already set.").note(format!("Old: {name}")).note(format!("New: {next}")));
								}
								name = next.into();
							}
							_ => return Err(eyre!("Unexpected flag `-{flag}` for %files")),
						}
					} else {
						if !name.is_empty() {
							return Err(eyre!("The name of the subpackage is already set.").note(format!("Old: {name}")).note(format!("New: {remain}")));
						}
						name = format!("{}-{remain}", self.rpm.name.as_ref().ok_or(eyre!("Expected package name before subpackage `{remain}`"))?).into();
					}
				}
				RPMSection::Files(name, f)
			}
			"changelog" => RPMSection::Changelog,
			_ => return Ok(false),
		};
		Ok(true)
	}

	/// Parses the spec file given as a [`io::BufReader`].
	///
	/// # Errors
	/// - Cannot expand macros ([`Self::_expand_macro()`])
	/// - Bad section syntax ([`Self::_handle_section()`])
	/// - Cannot detect arch ([`Self::arch()`])
	/// - Bad syntax in `Requires:` or other preambles
	/// - Other bad syntaxes
	///
	/// # Panic
	/// - The function expects a subpackage to be previously defined and created in
	///   `self.rpm.packages` and would panic if it was not found. This'd be a bug.
	pub fn parse<R: std::io::Read>(&mut self, bufread: BufReader<R>) -> Result<()> {
		let mut consumer = Consumer::new("", Some(bufread));
		while let Some(line) = consumer.read_til_eol() {
			let raw_line = self._expand_macro(&mut Consumer::from(&*line))?;
			let line = raw_line.trim();
			if line.is_empty() || line.starts_with('#') || RE_DNL.is_match(line) {
				continue;
			}
			if self._handle_section(line)? {
				continue;
			}
			// Check for Requires special preamble syntax first
			if matches!(self.section, RPMSection::Global) && self.parse_requires(line)? {
				continue;
			}
			if let RPMSection::Package(_) = self.section {
				if self.parse_requires(line)? {
					continue;
				}
			}
			match self.section {
				RPMSection::Global | RPMSection::Package(_) => {
					let Some(cap) = RE_PREAMBLE.captures(line) else {
						self.errors.push(ParserError::Others(eyre!("{}: Non-empty non-preamble line: {line}", self.count_line)));
						continue;
					};
					// check for list_preambles
					let Some(digitcap) = RE_DIGIT.captures(&cap[1]) else {
						self.add_preamble(&cap[1], cap[2].into())?;
						continue;
					};
					let strdigit = &digitcap[0];
					let digit = strdigit.parse()?;
					let name = &cap[1][..cap[1].len() - strdigit.len()];
					self.add_list_preamble(name, digit, &cap[2])?;
				}
				RPMSection::Description(ref p) => {
					if p.is_empty() {
						self.rpm.description.push_str(line);
						self.rpm.description.push('\n');
						continue;
					}
					let p = self.rpm.packages.get_mut(p).expect("BUG: no subpackage at %description");
					p.description.push_str(line);
					p.description.push('\n');
				}
				RPMSection::Prep => {
					self.rpm.prep.push_str(line);
					self.rpm.prep.push('\n');
				}
				RPMSection::Build => {
					self.rpm.build.push_str(line);
					self.rpm.build.push('\n');
				}
				RPMSection::Install => {
					self.rpm.install.push_str(line);
					self.rpm.install.push('\n');
				}
				RPMSection::Files(ref p, ref mut f) => {
					if let Some(f) = f {
						if p.is_empty() && self.rpm.files.incl.is_empty() {
							self.rpm.files.incl = take(f);
						} else {
							let p = self.rpm.packages.get_mut(p).expect("BUG: no subpackage at %files");
							if p.files.incl.is_empty() {
								p.files.incl = take(f);
							}
						}
					}
					if p.is_empty() {
						self.rpm.files.raw.push_str(line);
						self.rpm.files.raw.push('\n');
						continue;
					}
					let p = self.rpm.packages.get_mut(p).expect("BUG: no subpackage at %files");
					p.files.raw.push_str(line);
					p.files.raw.push('\n');
				}
				RPMSection::Changelog => {
					self.rpm.changelog.raw.push_str(line);
					self.rpm.changelog.raw.push('\n');
				}
			}
		}
		if !self.errors.is_empty() {
			println!("{:#?}", self.errors);
			return take(&mut self.errors).into_iter().fold(Err(eyre!("Cannot parse spec file")), color_eyre::Help::error);
		}
		self.rpm.changelog.parse()?;
		self.rpm.files.parse()?;
		self.rpm.packages.values_mut().try_for_each(|p| p.files.parse())?;
		Ok(())
	}

	/// Process and add `Source0:` and `Patch0:` preambles into `self.rpm`.
	///
	/// # Messages
	/// - If a preambled defined previously has been overridden, an error message will be given
	///   but parsing will continue:
	/// ```rpmspec
	/// Source0: ...
	/// Source1: again??? # error message from here
	/// ```
	///
	/// # Errors
	/// - The preamble is unknown / invalid
	pub fn add_list_preamble(&mut self, name: &str, digit: u32, value: &str) -> Result<()> {
		let value = value;
		let rpm = &mut self.rpm;
		macro_rules! no_override_ins {
			($attr:ident) => {{
				if let Some(old) = rpm.$attr.insert(digit, value.into()) {
					error!("Overriding preamble `{name}{digit}` value `{old}` -> `{value}`");
				}
			}};
		}
		match name {
			"Source" => no_override_ins!(sources),
			"Patch" => no_override_ins!(patches),
			_ => return Err(eyre!("Failed to match preamble `{name}{digit}` (value `{value}`)")),
		}
		Ok(())
	}

	// ! this function is impractical to be split.
	#[allow(clippy::cognitive_complexity)]
	/// Process and add preambles into `self.rpm` or subpackages.
	///
	/// List preambles which are defined in the format of `{preamble_name}{digit}` will NOT be
	/// processed here. See [`SpecParser::add_list_preamble`].
	///
	/// # Errors
	/// - Invalid dependency query ([`Package::add_query`], [`Package::add_simple_query`])
	/// - Cannot `parse()` string into boolean
	///
	/// # Panics
	/// ## Todo
	/// The following preambles are currently unimplemented and their implementations will be done later:
	/// - `OrderWithRequires`
	/// - `BuildConflicts`
	/// - `Prefixes`
	/// - `Prefix`
	/// - `DocDir`
	/// - `RemovePathPostfixes`
	pub fn add_preamble(&mut self, name: &str, value: String) -> Result<()> {
		let rpm = &mut self.rpm;

		macro_rules! opt {
			($x:ident $y:ident) => {
				if name == stringify!($x) {
					if let Some(ref old) = rpm.$y {
						warn!(
							"overriding existing {} preamble value `{old}` to `{value}`",
							stringify!($x)
						);
						self.errors.push(ParserError::Duplicate(self.count_line, stringify!($x).into()));
					}
					rpm.$y = Some(value);
					return Ok(());
				}
			};
			(~$x:ident $y:ident) => {
				if name == stringify!($x) {
					rpm.$y = value.parse()?;
					return Ok(());
				}
			};
			(%$x:ident $y:ident) => {
				if name == stringify!($x) {
					rpm.$y.append(&mut value.split_whitespace().map(|s| s.into()).collect());
					return Ok(());
				}
			};
			($a:ident $b:ident | $($x:ident $y:ident)|+) => {
				opt!($a $b);
				opt!($($x $y)|+);
			}
		}

		if let RPMSection::Package(ref pkg) = self.section {
			let rpm = rpm.packages.get_mut(pkg).expect("BUG: no subpackage in rpm.packages");
			match name {
				"Group" => {
					if let Some(ref old) = rpm.group {
						warn!("overriding existing Group preamble value `{old}` to `{value}`");
						self.errors.push(ParserError::Duplicate(self.count_line, "Group".into()));
					}
					rpm.name = Some(value);
					return Ok(());
				}
				"Summary" => {
					if !rpm.summary.is_empty() {
						warn!("overriding existing Summary preamble value `{}` to `{value}`", rpm.summary);
						self.errors.push(ParserError::Duplicate(self.count_line, "Summary".into()));
					}
					rpm.summary = value;
					return Ok(());
				}
				"Provides" => return Package::add_query(&mut rpm.provides, &value),
				"Obsoletes" => return Package::add_query(&mut rpm.obsoletes, &value),
				"Conflicts" => return Package::add_query(&mut rpm.conflicts, &value),
				"Suggests" => return Package::add_simple_query(&mut rpm.suggests, &value),
				"Recommends" => return Package::add_simple_query(&mut rpm.recommends, &value),
				"Enhances" => return Package::add_simple_query(&mut rpm.enhances, &value),
				"Supplements" => return Package::add_simple_query(&mut rpm.supplements, &value),
				_ => {} // get to global below
			}
		}

		opt!(Name name|Version version|Release release|License license|SourceLicense sourcelicense|URL url|BugURL bugurl|ModularityLabel modularitylabel|DistTag disttag|VCS vcs|Distribution distribution|Vendor vendor|Packager packager|Group group|Summary summary);
		opt!(~AutoReqProv autoreqprov);
		opt!(~AutoReq autoreq);
		opt!(~AutoProv autoprov);
		opt!(%ExcludeArch excludearch);
		opt!(%ExclusiveArch exclusivearch);
		opt!(%ExcludeOS exclusiveos);
		opt!(%ExclusiveOS exclusiveos);
		opt!(%BuildArch buildarch);
		opt!(%BuildArchitectures buildarch);

		match name {
			"Epoch" => {
				if let Some(old) = rpm.epoch {
					warn!("Overriding existing Epoch preamble value `{old}` to `{value}`");
				}
				rpm.epoch = Some(value.parse().expect("Failed to decode epoch to int"));
			}
			"Provides" => Package::add_query(&mut rpm.provides, &value)?,
			"Conflicts" => Package::add_query(&mut rpm.conflicts, &value)?,
			"Obsoletes" => Package::add_query(&mut rpm.obsoletes, &value)?,
			"Recommends" => Package::add_simple_query(&mut rpm.recommends, &value)?,
			"Suggests" => Package::add_simple_query(&mut rpm.suggests, &value)?,
			"Supplements" => Package::add_simple_query(&mut rpm.supplements, &value)?,
			"Enhances" => Package::add_simple_query(&mut rpm.enhances, &value)?,
			"BuildRequires" => Package::add_query(&mut rpm.buildrequires, &value)?,
			"OrderWithRequires" => todo!(),
			"BuildConflicts" => todo!(),
			"Prefixes" => todo!(),
			"Prefix" => todo!(),
			"DocDir" => todo!(),
			"RemovePathPostfixes" => todo!(),
			_ => self.errors.push(ParserError::UnknownPreamble(self.count_line, name.into())),
		}
		Ok(())
	}

	fn _internal_macro(&mut self, name: &str, reader: &mut Consumer) -> Option<String> {
		match name {
			"define" | "global" => {
				let def = reader.read_til_eol()?;
				let def = def.trim();
				let Some((name, def)) = def.split_once(' ') else {
					error!("Invalid syntax: `%define {def}`");
					return None
				};
				let mut def: String = def.into();
				let name: String = name.strip_suffix("()").map_or_else(
					|| name.into(),
					|x| {
						def.push(' ');
						x.into()
					},
				);
				self.macros.insert(name, def);
				Some("".into())
			}
			"undefine" => {
				self.macros.remove(name);
				Some("".into())
			}
			"load" => {
				self.load_macro_from_file(&std::path::PathBuf::from(&*reader.collect::<String>())).ok()?;
				Some("".into())
			}
			"expand" => self._expand_macro(reader).ok(),
			"expr" => unimplemented!(),
			"lua" => {
				let content: String = reader.collect();
				// HACK: `Arc<Mutex<SpecParser>>` as rlua fns are of `Fn` but they need `&mut SpecParser`.
				// HACK: The mutex needs to momentarily *own* `self`.
				let parser = Arc::new(Mutex::new(take(self)));
				let out = crate::lua::run(&parser, &content);
				std::mem::swap(self, &mut Arc::try_unwrap(parser).expect("Cannot unwrap Arc for print() output in lua").into_inner()); // break down Arc then break down Mutex
				match out {
					Ok(s) => Some(s.into()),
					Err(e) => {
						error!("%lua failed: {e:#}");
						None
					}
				}
			}
			"macrobody" => unimplemented!(),
			"quote" => unimplemented!(),
			"gsub" => unimplemented!(),
			"len" => Some(format!("{}", reader.collect::<Box<[char]>>().len()).into()),
			"lower" => Some(reader.collect::<String>().to_lowercase().into()),
			"rep" => unimplemented!(),
			"reverse" => unimplemented!(),
			"sub" => unimplemented!(),
			"upper" => unimplemented!(),
			"shescape" => unimplemented!(),
			"shrink" => unimplemented!(),
			"basename" => unimplemented!(),
			"dirname" => unimplemented!(),
			"exists" => unimplemented!(),
			"suffix" => unimplemented!(),
			"url2path" => unimplemented!(),
			"uncompress" => unimplemented!(),
			"getncpus" => unimplemented!(),
			"getconfidir" => unimplemented!(),
			"getenv" => unimplemented!(),
			"rpmversion" => unimplemented!(),
			"echo" => {
				println!("{}", reader.collect::<String>());
				Some("".into())
			}
			"warn" => {
				warn!("{}", reader.collect::<String>());
				Some("".into())
			}
			"error" => {
				error!("{}", reader.collect::<String>());
				Some("".into())
			}
			"verbose" => unimplemented!(),
			"S" => unimplemented!(),
			"P" => unimplemented!(),
			"trace" => {
				trace!("{}", reader.collect::<String>());
				Some("".into())
			}
			"dump" => unimplemented!(),
			_ => None,
		}
	}

	/// parses:
	/// ```rpmspec
	/// %macro_name -a -b hai bai idk \
	///   more args idk
	/// ```
	/// but not:
	/// ```rpmspec
	/// %{macro_name:hai bai -f -a}
	/// ```
	fn _param_macro_line_args(&mut self, reader: &mut Consumer) -> Result<(String, Vec<String>, Vec<char>)> {
		// we start AFTER %macro_name
		let mut content = String::new();
		let mut flags = vec![];
		let (mut pa, mut pb, mut pc, mut sq, mut dq);
		gen_read_helper!(reader pa pb pc sq dq);
		macro_rules! exit {
			() => {
				exit_chk!();
				let args = content.split(' ').filter(|x| !x.starts_with('-')).map(|x| x.into()).collect();
				return Ok((content, args, flags));
			};
		}
		'main: while let Some(ch) = reader.next() {
			if ch == '%' {
				let ch = next!(~'%');
				if ch == '%' {
					content.push('%');
					continue;
				}
				reader.push(ch);
				content.push_str(&self._read_raw_macro_use(reader)?);
				continue;
			}
			if ch == '-' {
				let ch = next!(~'-');
				if !ch.is_ascii_alphabetic() {
					return Err(eyre!("Argument flag `-{ch}` in parameterized macro is not alphabetic"));
				}
				let next = next!(#);
				if !"\\ \n".contains(next) {
					return Err(eyre!("Found character `{next}` after `-{ch}` in parameterized macro"));
				}
				reader.push(next);
				flags.push(ch);
				content.push('-');
				content.push(ch);
				continue;
			}
			if ch == '\\' {
				let mut got_newline = false;
				content = content.trim_end().into();
				content.push(' ');
				while let Some(ch) = reader.next() {
					if ch == '\n' {
						got_newline = true;
					} else if !ch.is_whitespace() {
						if got_newline {
							reader.push(ch);
							continue 'main;
						}
						return Err(eyre!("Got `{ch}` after `\\` before new line"));
					}
				}
				return Err(eyre!("Unexpected EOF after `\\`"));
			}
			if ch == '\n' && !quote_remain!() {
				break;
			}
			chk_ps!(ch);
			// compress whitespace to ' '
			if ch.is_whitespace() && content.chars().last().map_or(false, |l| !l.is_whitespace()) {
				content.push(' ');
			} else if !ch.is_whitespace() {
				content.push(ch);
			}
		}
		exit!();
	}

	fn _param_macro(&mut self, name: &str, def: &mut Consumer, reader: &mut Consumer) -> Result<String> {
		let (raw_args, args, flags) = self._param_macro_line_args(reader)?;
		let mut res = String::new();
		let (mut pa, mut pb, mut pc, mut sq, mut dq);
		gen_read_helper!(def pa pb pc sq dq);
		macro_rules! exit {
			() => {
				exit_chk!();
				return Ok("".into());
			};
		}
		'main: while let Some(ch) = def.next() {
			if ch != '%' {
				chk_ps!(ch);
				res.push(ch);
				continue;
			}
			let ch = next!(~'%'); // will chk_ps after `%` chk
			if ch == '%' {
				res.push('%');
				continue;
			}
			chk_ps!(ch);
			// ? https://rpm-software-management.github.io/rpm/manual/macros.html
			match ch {
				'*' => {
					let follow = next!(~'*');
					if follow == '*' {
						res.push_str(&raw_args); // %**
					} else {
						def.push(follow);
						res.push_str(&args.join(" ")); // %*
					}
				}
				'#' => res.push_str(&args.len().to_string()),
				'0' => res.push_str(name),
				'{' => {
					let req_pc = pc - 1;
					let mut content = String::new();
					def.take_while(|ch| {
						// find `}`
						chk_ps!(ch);
						if req_pc != pc {
							content.push(*ch);
						}
						req_pc != pc
					})
					.for_each(|_| {}); // do nothing
					if req_pc != pc {
						return Err(eyre!("Unexpected EOF while parsing `%{{...`"));
					}
					#[allow(clippy::option_if_let_else)] // WARN refactor fail count: 2
					let notflag = if let Some(x) = content.strip_prefix('!') {
						content = x.into();
						true
					} else {
						false
					};
					let expand = {
						let binding = content.clone();
						if let Some((name, e)) = binding.split_once(':') {
							content = name.to_string().into();
							e.into()
						} else {
							binding
						}
					};
					if !content.starts_with('-') {
						// normal %macros
						res.push_str(&self._read_raw_macro_use(&mut Consumer::from(&*format!("{{{content}}}")))?);
						continue 'main;
					}
					if let Some(content) = content.strip_suffix('*') {
						if content.len() != 2 {
							return Err(eyre!("Invalid macro param flag `%{{{content}}}`"));
						}
						let mut args = raw_args.split(' ');
						if !notflag {
							if let Some(n) = args.clone().enumerate().find_map(|(n, x)| if x == content { Some(n) } else { None }) {
								if let Some(arg) = args.nth(n + 1) {
									res.push_str(arg);
								}
							}
						}
						continue 'main; // no args after -f, add nothing.
					}
					if content.len() != 2 {
						return Err(eyre!("Found `%-{content}` which is not a flag"));
					}
					let flag = unsafe { content.chars().last().unwrap_unchecked() };
					if !flag.is_ascii_alphabetic() {
						return Err(eyre!("Invalid macro name `%-{flag}`"));
					}
					if flags.contains(&flag) ^ notflag {
						res.push_str(&expand);
					}
					continue 'main;
				}
				_ if ch.is_numeric() => {
					let mut macroname = String::new();
					macroname.push(ch);
					// no need chk_ps!(), must be numeric
					while let Some(ch) = def.next() {
						if !ch.is_numeric() {
							def.push(ch);
							break;
						}
						macroname.push(ch);
					}
					res.push_str(
						match macroname.parse::<usize>() {
							Ok(n) => args.get(n - 1),
							Err(e) => return Err(eyre!("Cannot parse macro param `%{macroname}`: {e}")),
						}
						.unwrap_or(&String::new()),
					);
				}
				_ => {
					def.push(ch);
					res.push_str(&self._read_raw_macro_use(def)?);
				}
			}
		}
		exit_chk!();
		Ok(res)
	}

	fn _expand_macro(&mut self, chars: &mut Consumer) -> Result<String> {
		let mut res = String::new();
		while let Some(ch) = chars.next() {
			self.count_chard += 1;
			self.count_col += 1;
			if ch == '\n' {
				self.count_line += 1;
				self.count_col = 0;
			}
			if ch == '%' {
				if let Some(ch) = chars.next() {
					if ch == '%' {
						res.push('%');
						continue;
					}
					chars.push(ch);
					res.push_str(&self._read_raw_macro_use(chars).wrap_err_with(|| format!("Cannot parse macro ({}:{})", self.count_line, self.count_col))?);
					continue;
				}
			}
			res.push(ch);
		}
		Ok(res)
	}

	fn _rp_macro(&mut self, name: &str, reader: &mut Consumer) -> Result<String> {
		debug!("getting %{name}");
		if let Some(expanded) = self._internal_macro(name, reader) {
			return Ok(expanded);
		}
		if let Some(def) = self.macros.get(name) {
			// Refactor at your own risk: impossible due to RAII. Fail counter: 2
			if let Some(def) = def.strip_suffix(' ') {
				// parameterized macro
				let out = self._param_macro(name, &mut Consumer::from(def), reader)?;
				self._expand_macro(&mut Consumer::from(&*out))
			} else {
				// it's just text
				self._expand_macro(&mut Consumer::from(def.as_str()))
			}
		} else {
			Err(eyre!("Macro not found: {name}"))
		}
	}

	/// parse the stuff after %, and determines `{[()]}`. returns expanded macro.
	/// FIXME please REFACTOR me!!
	// * when %a is undefined, %{!a} expands to %{!a}, but %!a expands to %a
	pub(crate) fn _read_raw_macro_use(&mut self, chars: &mut Consumer) -> Result<String> {
		debug!("reading macro");
		let (mut notflag, mut question) = (false, false);
		let mut content = String::new();
		let mut first = true;
		let (mut pa, mut pb, mut pc, mut sq, mut dq);
		gen_read_helper!(chars pa pb pc sq dq);
		while let Some(ch) = chars.next() {
			chk_ps!(ch); // we read until we encounter '}' or ':' or the end
			match ch {
				'!' => notflag = !notflag,
				'?' => {
					if question {
						warn!("Seeing double `?` flag in macro use. Ignoring.");
					}
					question = true;
				}
				'{' => {
					let req_pc = pc - 1;
					if !first {
						back!('{');
						break;
					}
					let mut name = String::new();
					while let Some(ch) = chars.next() {
						chk_ps!(ch);
						if pc == req_pc {
							exit_chk!();
							let out = self._rp_macro(&name, chars);
							if notflag {
								if question {
									return Ok("".into());
								}
								return Ok(out.unwrap_or_else(|e| {
									debug!("_rp_macro: {e:#}");
									if content.is_empty() { format!("%{name}") } else { format!("%{{!{name}}}") }.into()
								}));
							}
							return Ok(out.unwrap_or_default());
						}
						if ch == ':' {
							let mut content = String::new();
							for ch in chars.by_ref() {
								chk_ps!(ch);
								if pc == req_pc {
									if question {
										return Ok(if self.macros.contains_key(&name) ^ notflag { self.parse_macro(&mut (&*content).into()).collect() } else { "".into() });
									}
									exit_chk!();
									let out = self._rp_macro(&name, &mut (&*content).into());
									if notflag {
										if question {
											return Ok("".into());
										}
										// when %a is undefined, %{!a} expands to %{!a}, but %!a expands to %a
										return Ok(out.unwrap_or_else(|e| {
											debug!("_rp_macro: {e:#}");
											if content.is_empty() { format!("%{name}") } else { format!("%{{!{name}}}") }.into()
										}));
									}
									return Ok(out.unwrap_or_default());
								}
								content.push(ch);
							}
							return Err(eyre!("EOF but `%{{...:...` is not ended with `}}`"));
						}
						if ch == '?' {
							if question {
								warn!("Seeing double `?` flag in macro use. Ignoring.");
							}
							question = true;
							continue;
						}
						if ch == '!' {
							notflag = !notflag;
							continue;
						}
						name.push(ch);
					}
					return Err(eyre!("EOF while parsing `%{{...`"));
				}
				'(' => {
					if !content.is_empty() {
						back!('(');
						break;
					}
					if notflag || question {
						error!("flags (! and ?) are not supported for %().");
					}
					let mut shellcmd = String::new();
					let req_pa = pa - 1;
					for ch in chars.by_ref() {
						chk_ps!(ch);
						if pa == req_pa {
							return Err(match Command::new("sh").arg("-c").arg(&*shellcmd).output() {
								Ok(out) if out.status.success() => return Ok(core::str::from_utf8(&out.stdout)?.trim_end_matches('\n').into()),
								Ok(out) => eyre!("Shell expansion command did not succeed")
									.note(out.status.code().map_or("No status code".into(), |c| format!("Status code: {c}")))
									.section(std::string::String::from_utf8(out.stdout)?.header("Stdout:"))
									.section(std::string::String::from_utf8(out.stderr)?.header("Stderr:")),
								Err(e) => eyre!(e).wrap_err("Shell expansion failed"),
							})
							.note(shellcmd);
						}
						shellcmd.push(ch);
					}
					return Err(eyre!("Unexpected end of shell expansion command: `%({shellcmd}`"));
				}
				// '[' => todo!("what does %[] mean? www"),
				_ if !(ch.is_ascii_alphanumeric() || ch == '_') => {
					back!(ch);
					break;
				}
				_ => {}
			}
			first = false;
			content.push(ch);
		}
		exit_chk!();
		if notflag && question {
			return Ok("".into());
		}
		let out = self._rp_macro(&content, chars);
		if notflag {
			return Ok(out.unwrap_or_else(|e| {
				debug!("_rp_macro: {e:#}");
				// when %a is undefined, %{!a} expands to %{!a}, but %!a expands to %a
				if content.is_empty() { format!("%{content}") } else { format!("%{{!{content}}}") }.into()
			}));
		}
		Ok(out.unwrap_or_default())
	}

	/// Creates a new RPM spec parser.
	#[must_use]
	pub fn new() -> Self {
		Self { rpm: RPMSpec::new(), errors: vec![], macros: HashMap::new(), ..Self::default() }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs::File;

	#[test]
	fn parse_spec() -> Result<()> {
		let f = File::open("../tests/test.spec")?;
		let f = BufReader::new(f);

		let mut sp = SpecParser::new();
		sp.parse(f)?;
		println!("Name: {}", sp.rpm.name.unwrap_or_default());
		println!("Summary: {}", sp.rpm.summary.unwrap_or_default());
		Ok(())
	}
	#[test]
	fn test_load_macros() -> Result<()> {
		println!("{}", SpecParser::arch()?);
		let mut sp = SpecParser::new();
		sp.load_macros()?;
		println!("{:#?}", sp.macros);
		Ok(())
	}
	#[test]
	fn simple_macro_expand() -> Result<()> {
		let mut parser = super::SpecParser::new();
		parser.macros.insert("macrohai".into(), "hai hai".into());
		assert_eq!(parser._read_raw_macro_use(&mut ("macrohai".into()))?, "hai hai");
		Ok(())
	}
	#[test]
	fn text_recursive_macro_expand() -> Result<()> {
		let mut parser = super::SpecParser::new();
		parser.macros.insert("mhai".into(), "hai hai".into());
		parser.macros.insert("quadhai".into(), "%mhai %{mhai}".into());
		assert_eq!(parser._read_raw_macro_use(&mut ("quadhai".into()))?, "hai hai hai hai");
		Ok(())
	}
	#[test]
	fn text_quoting_recursive_macro_expand() -> Result<()> {
		let mut parser = super::SpecParser::new();
		parser.macros.insert("mhai".into(), "hai hai".into());
		parser.macros.insert("idk".into(), "%!?mhai %?!mhai %{mhai}".into());
		parser.macros.insert("idk2".into(), "%{?mhai} %{!mhai} %{!?mhai} %{?!mhai}".into());
		parser.macros.insert("aaa".into(), "%idk %idk2".into());
		assert_eq!(parser._read_raw_macro_use(&mut ("aaa".into()))?, "  hai hai hai hai hai hai  ");
		Ok(())
	}
	#[test]
	fn shell_macro_expand() -> Result<()> {
		let mut parser = super::SpecParser::new();
		parser.macros.insert("x".into(), "%(echo haai | sed 's/a/aa/g')".into());
		assert_eq!(parser._read_raw_macro_use(&mut ("x".into()))?, "haaaai");
		Ok(())
	}
	#[test]
	fn presence_macro_expand() -> Result<()> {
		let mut parser = super::SpecParser::new();
		parser.macros.insert("x".into(), "%{?not_exist:hai}%{!?not_exist:bai}".into());
		assert_eq!(parser._read_raw_macro_use(&mut ("x".into()))?, "bai");
		parser.macros.insert("not_exist".into(), "wha".into());
		assert_eq!(parser._read_raw_macro_use(&mut ("x".into()))?, "hai");
		Ok(())
	}
	#[test]
	fn param_macro_args_parsing() -> Result<()> {
		let mut parser = super::SpecParser::new();
		assert_eq!(
			parser._param_macro_line_args(&mut Consumer::from("-a hai -b asdfsdklj \\  \n abcd\ne"))?,
			("-a hai -b asdfsdklj abcd".into(), vec!["hai".into(), "asdfsdklj".into(), "abcd".into()], vec!['a', 'b'])
		);
		Ok(())
	}
	#[test]
	fn param_macro_expand() {
		let mut p = super::SpecParser::new();
		p.macros.insert("hai".into(), "hai, %1! ".into());
		assert_eq!(p.parse_macro(&mut "%hai madomado".into()).collect::<String>(), String::from("hai, madomado!"));
	}
}
