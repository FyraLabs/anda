#![warn(clippy::disallowed_types)]
use crate::error::ParserError;
use crate::util::*;
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
use tracing::{debug, error, warn};

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

#[derive(Default, Clone, Copy, Debug)]
pub enum PkgQCond {
	#[default]
	Eq, // =
	Le, // <=
	Lt, // <
	Ge, // >=
	Gt, // >
}

impl From<&str> for PkgQCond {
	fn from(value: &str) -> Self {
		match value {
			"=" => PkgQCond::Eq,
			">=" => PkgQCond::Ge,
			">" => PkgQCond::Gt,
			"<=" => PkgQCond::Le,
			"<" => PkgQCond::Lt,
			_ => unreachable!("Regex RE_PKGQCOND matched bad condition `{value}`"),
		}
	}
}

#[derive(Clone, Default, Debug)]
pub struct Package {
	pub name: String,
	pub version: Option<String>,
	pub release: Option<String>,
	pub epoch: Option<u32>,
	pub condition: PkgQCond,
}

impl Package {
	pub fn new(name: String) -> Self {
		Self { name, ..Self::default() }
	}
	// Simple query: query without the <= and >= and versions and stuff. Only names.
	pub fn add_simple_query(pkgs: &mut Vec<Self>, query: &str) -> Result<()> {
		let query = query.trim();
		let mut last = String::new();
		for ch in query.chars() {
			if ch == ' ' || ch == ',' {
				pkgs.push(Package::new(std::mem::take(&mut last)));
				continue;
			}
			if ch.is_alphanumeric() || PKGNAMECHARSET.contains(ch) {
				return Err(eyre!("Invalid character `{ch}` found in package query.").note(format!("query: `{query}`")));
			}
			last.push(ch);
		}
		if !last.is_empty() {
			pkgs.push(Package::new(last));
		}
		Ok(())
	}
	pub fn add_query(pkgs: &mut Vec<Self>, query: &str) -> Result<()> {
		let query = query.trim(); // just in case
		let Some((name, rest)) = query.split_once(|c: char| !c.is_alphanumeric() && !PKGNAMECHARSET.contains(c)) else {
			// check if query matches pkg name
			if query.chars().any(|c| !c.is_alphanumeric() && !PKGNAMECHARSET.contains(c)) {
				return Err(eyre!("Invalid package name `{query}`").suggestion("Use only alphanumerics, underscores and dashes."));
			}
			pkgs.push(Self::new(query.into()));
			return Ok(())
		};
		// the part that matches the good name is `name`. Check the rest.
		let mut pkg = Package::new(name.into());
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

#[derive(Default, Clone, Debug)]
pub struct RPMRequires {
	pub none: Vec<Package>,
	pub pre: Vec<Package>,
	pub post: Vec<Package>,
	pub preun: Vec<Package>,
	pub postun: Vec<Package>,
	pub pretrans: Vec<Package>,
	pub posttrans: Vec<Package>,
	pub verify: Vec<Package>,
	pub interp: Vec<Package>,
	pub meta: Vec<Package>,
}

#[derive(Default, Clone, Debug)]
pub struct Scriptlets {
	pub pre: Option<String>,
	pub post: Option<String>,
	pub preun: Option<String>,
	pub postun: Option<String>,
	pub pretrans: Option<String>,
	pub posttrans: Option<String>,
	pub verify: Option<String>,

	pub triggerprein: Option<String>,
	pub triggerin: Option<String>,
	pub triggerun: Option<String>,
	pub triggerpostun: Option<String>,

	pub filetriggerin: Option<String>,
	pub filetriggerun: Option<String>,
	pub filetriggerpostun: Option<String>,
	pub transfiletriggerin: Option<String>,
	pub transfiletriggerun: Option<String>,
	pub transfiletriggerpostun: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub enum ConfigFileMod {
	#[default]
	None,
	MissingOK,
	NoReplace,
}

#[derive(Debug, Clone)]
pub enum VerifyFileMod {
	FileDigest, // or 'md5'
	Size,
	Link,
	User, // or 'owner'
	Group,
	Mtime,
	Mode,
	Rdev,
	Caps,
}

#[derive(Default, Debug, Clone)]
pub struct RPMFile {
	// %artifact
	pub artifact: bool,
	// %ghost
	pub ghost: bool,
	// %config
	pub config: ConfigFileMod,
	// %dir
	pub dir: bool,
	// %readme (obsolete) = %doc
	// %doc
	pub doc: bool,
	// %license
	pub license: bool,
	// %verify
	pub verify: Option<VerifyFileMod>,

	pub path: String,
	pub mode: u16,
	pub user: String,
	pub group: String,
}

#[derive(Default, Clone, Debug)]
pub struct RPMFiles {
	pub incl: String,
	pub files: Box<[RPMFile]>,
	pub raw: String,
}

impl RPMFiles {
	fn parse(&mut self) -> Result<()> {
		//? http://ftp.rpm.org/max-rpm/s1-rpm-inside-files-list-directives.html
		self.files = RE_FILE
			.captures_iter(&self.raw)
			.map(|cap| {
				let mut f = RPMFile::default();
				if let Some(name) = cap.get(1) {
					let name = name.as_str();
					if let Some(m) = cap.get(2) {
						let x = m.as_str().strip_prefix('(').expect("RE_FILE not matching parens `(...)` but found capture group 2");
						let x = x.strip_suffix(')').expect("RE_FILE not matching parens `(...)` but found capture group 2");
						let ss: Vec<&str> = x.split(',').map(|s| s.trim()).collect();
						if name.starts_with("%attr(") {
							let Some([mode, user, group]) = ss.get(0..=2) else {
								return Err(eyre!("Expected 3 arguments in `%attr(...)`"));
							};
							let (mode, user, group) = (*mode, *user, *group);
							if mode != "-" {
								f.mode = mode.parse().map_err(|e: ParseIntError| eyre!(e).wrap_err("Cannot parse file mode"))?;
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
							todo!()
						}
						if name.starts_with("%defattr(") {
							todo!()
						}
						if name.starts_with("%config(") {
							todo!()
						}
						return Err(eyre!("Unknown %files directive: %{name}"));
					}
					match name {
						"%artifact " => f.artifact = true,
						"%ghost " => f.ghost = true,
						"%config " => f.config = ConfigFileMod::MissingOK,
						"%dir " => f.dir = true,
						"%doc " => f.doc = true,
						"%readme " => f.doc = true,
						"%license " => f.license = true,
						_ => return Err(eyre!("Unknown %files directive: %{name}")),
					}
				}
				f.path = cap.get(3).expect("No RE grp 3 in %files?").as_str().into();
				Ok(f)
			})
			.collect::<Result<Box<[RPMFile]>>>()?;
		Ok(())
	}
}

#[derive(Default, Clone, Debug)]
pub struct Changelog {
	pub date: chrono::NaiveDate,
	pub version: Option<String>,
	pub maintainer: String,
	pub email: Option<String>,
	pub message: String,
}

#[derive(Default, Clone, Debug)]
pub struct Changelogs {
	pub changelogs: Box<[Changelog]>,
	pub raw: String,
}

impl Changelogs {
	fn parse(&mut self) -> Result<()> {
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

#[derive(Default, Clone, Debug)]
pub enum RPMSection {
	#[default]
	Global,
	Package(String),
	Description(String),
	Prep,
	Build,
	Install,
	Files(String, Option<String>),
	Changelog,
}

#[derive(Default, Clone, Debug)]
pub(crate) struct RPMSpecPkg {
	pub name: Option<String>,
	// pub version: Option<String>,
	// pub release: Option<String>,
	// pub epoch: Option<i32>,
	pub summary: String,
	// pub buildarch: String,
	pub requires: RPMRequires,
	pub description: String,
	pub group: Option<String>,
	pub provides: Vec<Package>,
	pub conflicts: Vec<Package>,
	pub obsoletes: Vec<Package>,
	pub recommends: Vec<Package>,
	pub suggests: Vec<Package>,
	pub supplements: Vec<Package>,
	pub enhances: Vec<Package>,
	pub files: RPMFiles,
}

#[derive(Default, Clone, Debug)]
pub struct RPMSpec {
	pub(crate) packages: HashMap<String, RPMSpecPkg>,

	// %description
	pub description: String,
	// %prep
	pub prep: String,
	// %generate_buildrequires
	pub generate_buildrequires: Option<String>,
	// %conf
	pub conf: Option<String>,
	// %build
	pub build: String,
	// %install
	pub install: String,
	// %check
	pub check: String,

	pub scriptlets: Scriptlets,
	pub files: RPMFiles,
	pub changelog: Changelogs,

	//* preamble
	pub name: Option<String>,
	pub version: Option<String>,
	pub release: Option<String>,
	pub epoch: Option<i32>,
	pub license: Option<String>,
	pub sourcelicense: Option<String>,
	pub group: Option<String>,
	pub summary: Option<String>,
	pub sources: HashMap<i16, String>,
	pub patches: HashMap<i16, String>,
	// TODO icon
	// TODO nosource nopatch
	pub url: Option<String>,
	pub bugurl: Option<String>,
	pub modularitylabel: Option<String>,
	pub disttag: Option<String>,
	pub vcs: Option<String>,
	pub distribution: Option<String>,
	pub vendor: Option<String>,
	pub packager: Option<String>,
	// TODO buildroot
	pub autoreqprov: bool,
	pub autoreq: bool,
	pub autoprov: bool,
	pub requires: RPMRequires,
	pub provides: Vec<Package>,
	pub conflicts: Vec<Package>,
	pub obsoletes: Vec<Package>,
	pub recommends: Vec<Package>,
	pub suggests: Vec<Package>,
	pub supplements: Vec<Package>,
	pub enhances: Vec<Package>,
	pub orderwithrequires: Vec<Package>,
	pub buildrequires: Vec<Package>,
	pub buildconflicts: Vec<Package>,
	pub excludearch: Vec<String>,
	pub exclusivearch: Vec<String>,
	pub excludeos: Vec<String>,
	pub exclusiveos: Vec<String>,
	pub buildarch: Vec<String>, // BuildArchitectures BuildArch
	pub prefix: Option<String>, // Prefixes Prefix
	pub docdir: Option<String>,
	pub removepathpostfixes: Vec<String>,
}

impl RPMSpec {
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

#[derive(Default, Clone, Debug)]
pub struct SpecParser {
	pub rpm: RPMSpec,
	errors: Vec<ParserError>,
	pub macros: HashMap<String, String>,
	section: RPMSection,
	cond: Vec<(bool, bool)>, // current, before

	pub(crate) count_line: usize,
	pub(crate) count_col: usize,
	pub(crate) count_chard: usize,
}

impl SpecParser {
	pub fn parse_macro<'a>(&'a mut self, reader: &'a mut Consumer) -> SpecMacroParserIter {
		SpecMacroParserIter { reader, parser: self, percent: false, buf: String::new() }
	}

	pub fn parse_requires(&mut self, sline: &str) -> Result<bool> {
		let Some(caps) = RE_REQ1.captures(sline) else {
			return Ok(false);
		};
		let mut pkgs = vec![];
		Package::add_query(&mut pkgs, caps[2].trim())?;
		let modifiers = if caps.len() == 2 { &caps[2] } else { "none" };
		for modifier in modifiers.split(',') {
			let modifier = modifier.trim();
			let pkgs = pkgs.to_vec();
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
	pub fn arch() -> Result<String> {
		let binding = Command::new("uname").arg("-m").output()?;
		let s = core::str::from_utf8(&binding.stdout)?;
		Ok(s[..s.len() - 1].into()) // remove new line
	}

	// todo rewrite
	pub fn load_macro_from_file(&mut self, path: std::path::PathBuf) -> Result<()> {
		lazy_static::lazy_static! {
			static ref RE: Regex = Regex::new(r"(?m)^%([\w()]+)[\t ]+((\\\n|[^\n])+)$").unwrap();
		}
		debug!("Loading macros from {}", path.display());
		let mut buf = vec![];
		let bytes = BufReader::new(std::fs::File::open(&path)?).read_to_end(&mut buf)?;
		assert_ne!(bytes, 0, "Empty macro definition file '{}'", path.display());
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

	// not sure where I've seen the docs, but there was one lying around saying you can define multiple
	// macros with the same name, and when you undefine it the old one recovers (stack?). I don't think
	// it is a good idea to do it like that (it is simply ridiculous and inefficient) but you can try
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
				self.load_macro_from_file(path?)?;
			}
		}
		Ok(())
	}

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
		self.section = match &start[1..] {
			"if" => {
				let c = remain.parse().map_or(true, |n: isize| n != 0);
				self.cond.push((c, c));
				return Ok(true);
			}
			"ifarch" => {
				let c = remain == Self::arch()?;
				self.cond.push((c, c));
				return Ok(true);
			}
			"ifnarch" => {
				let c = remain != Self::arch()?;
				self.cond.push((c, c));
				return Ok(true);
			}
			"elifarch" => {
				let Some((a, b)) = self.cond.last_mut() else { return Err(eyre!("%elifarch found without %if/%ifarch"))};
				if *b {
					*a = false;
				} else {
					*a = remain == Self::arch()?;
					*b = *a;
				}
				return Ok(true);
			}
			"elifnarch" => {
				let Some((a, b)) = self.cond.last_mut() else { return Err(eyre!("%elifarch found without %if/%ifarch"))};
				if *b {
					*a = false;
				} else {
					*a = remain != Self::arch()?;
					*b = *a;
				}
				return Ok(true);
			}
			"elif" => {
				let Some((a, b)) = self.cond.last_mut() else {return Err(eyre!("%elif found without %if"))};
				if *b {
					*a = false;
				} else {
					*a = remain.parse().map_or(true, |n: isize| n != 0);
					*b = *a;
				}
				return Ok(true);
			}
			"else" => {
				let Some((a, b)) = self.cond.last_mut() else {return Err(eyre!("%else found without %if"))};
				if *b {
					*a = false;
				} else {
					*a = !(*a);
					// *b = *a; (doesn't matter)
				}
				return Ok(true);
			}
			"endif" => {
				return if self.cond.pop().is_none() { Err(eyre!("%endif found without %if")) } else { Ok(true) };
			}
			"description" => RPMSection::Description(if !remain.is_empty() {
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
			} else {
				"".into()
			}),
			"package" => {
				if remain.is_empty() {
					return Err(eyre!("Expected arguments to %package"));
				}
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

	pub fn parse<R: std::io::Read>(&mut self, bufread: BufReader<R>) -> Result<()> {
		let mut consumer = Consumer::new("".into(), Some(bufread));
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
			if let RPMSection::Global = self.section {
				if self.parse_requires(line)? {
					continue;
				}
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
					let sdigit = &digitcap[0];
					let digit = sdigit.parse()?;
					let name = &cap[1][..cap[1].len() - sdigit.len()];
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
			return take(&mut self.errors).into_iter().fold(Err(eyre!("Cannot parse spec file")), |report, e| report.error(e));
		}
		self.rpm.changelog.parse()?;
		self.rpm.files.parse()?;
		self.rpm.packages.values_mut().try_for_each(|p| p.files.parse())?;
		Ok(())
	}

	pub fn add_list_preamble(&mut self, name: &str, digit: i16, value: &str) -> Result<()> {
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
					rpm.name = Some(value.parse()?);
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
				let name: String = if let Some(x) = name.strip_suffix("()") {
					def.push(' ');
					x.into()
				} else {
					name.into()
				};
				self.macros.insert(name, def);
				Some("".into())
			}
			"undefine" => {
				self.macros.remove(name);
				Some("".into())
			}
			"load" => {
				self.load_macro_from_file(std::path::PathBuf::from(&*reader.collect::<String>())).ok()?;
				Some("".into())
			}
			"expand" => self._expand_macro(reader).ok(),
			"expr" => unimplemented!(),
			"lua" => {
				let content: String = reader.collect();
				// HACK: `Arc<Mutex<SpecParser>>` as rlua fns are of `Fn` but they need `&mut SpecParser`.
				// HACK: The mutex needs to momentarily *own* `self`.
				let parser = Arc::new(Mutex::new(take(self)));
				let out = crate::lua::RPMLua::run(parser.clone(), &content);
				std::mem::swap(self, &mut Arc::try_unwrap(parser).unwrap().into_inner()); // break down Arc then break down Mutex
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
			"len" => unimplemented!(),
			"lower" => unimplemented!(),
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
			"echo" => unimplemented!(),
			"warn" => unimplemented!(),
			"error" => unimplemented!(),
			"verbose" => unimplemented!(),
			"S" => unimplemented!(),
			"P" => unimplemented!(),
			"trace" => unimplemented!(),
			"dump" => unimplemented!(),
			_ => None,
		}
	}

	/// parses:
	/// ```
	/// %macro_name -a -b hai bai idk \
	///   more args idk
	/// ```
	/// but not:
	/// ```
	/// %{macro_name:hai bai -f -a}
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
			chk_ps!(ch);
			if ch == '%' {
				let ch = next!('%');
				if ch == '%' {
					content.push('%');
					continue;
				}
				back!(ch);
				content.push_str(&self._read_raw_macro_use(reader)?);
				continue;
			}
			if ch == '-' {
				let ch = next!('-');
				if !ch.is_ascii_alphabetic() {
					return Err(eyre!("Argument flag `-{ch}` in parameterized macro is not alphabetic"));
				}
				let next = next!(ch);
				if !"\\ \n".contains(next) {
					return Err(eyre!("Found character `{next}` after `-{ch}` in parameterized macro"));
				}
				back!(next);
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
					chk_ps!(ch);
					if ch == '\n' {
						got_newline = true;
					} else if !ch.is_whitespace() {
						if got_newline {
							back!(ch);
							continue 'main;
						}
						return Err(eyre!("Got `{ch}` after `\\` before new line"));
					}
				}
				error!("Unexpected EOF after `\\`");
			}
			if ch == '\n' && !quote_remain!() {
				exit!();
			}
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
			// for gen_read_helper!()
			() => {
				exit_chk!();
				return Ok("".into());
			};
		}
		'main: while let Some(ch) = def.next() {
			chk_ps!(ch);
			if ch != '%' {
				res.push(ch);
				continue;
			}
			let ch = next!('%');
			if ch == '%' {
				res.push('%');
				continue;
			}
			// https://rpm-software-management.github.io/rpm/manual/macros.html
			match ch {
				'*' => {
					let follow = next!('*');
					if follow == '*' {
						res.push_str(&raw_args); // %**
					} else {
						back!(follow);
						res.push_str(&args.join(" ")); // %*
					}
					continue;
				}
				'#' => {
					res.push_str(&args.len().to_string());
					continue;
				}
				'0' => {
					res.push_str(name);
					continue;
				}
				'{' => {
					let req_pc = pc - 1;
					let mut content = String::new();
					for ch in def.by_ref() {
						chk_ps!(ch);
						if req_pc != pc {
							content.push(ch);
							continue;
						}
						// found `}`
						let mut notflag = false;
						if let Some(x) = content.strip_prefix('!') {
							notflag = true;
							content = x.into();
						}
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
							let mut argv = raw_args.split(' ');
							if !notflag {
								if let Some(n) = argv.clone().enumerate().find_map(|(n, x)| if x == content { Some(n) } else { None }) {
									if let Some(arg) = argv.nth(n + 1) {
										res.push_str(arg);
									}
								}
							}
							// if there are no args after -f, add nothing.
							continue 'main;
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
					return Err(eyre!("Unexpected EOF while parsing `%{{...`"));
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
					let ret = match macroname.parse::<usize>() {
						Ok(n) => args.get(n - 1),
						Err(e) => {
							return Err(eyre!("Cannot parse macro param `%{macroname}`: {e}"));
						}
					};
					res.push_str(ret.unwrap_or(&String::new()));
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
	pub(crate) fn _read_raw_macro_use(&mut self, chars: &mut Consumer) -> Result<String> {
		debug!("reading macro");
		let (mut notflag, mut question) = (false, false);
		let mut content = String::new();
		let mut first = true;
		let (mut pa, mut pb, mut pc, mut sq, mut dq);
		gen_read_helper!(chars pa pb pc sq dq);
		while let Some(ch) = chars.next() {
			chk_ps!(ch);
			// we read until we encounter '}' or ':' or the end
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
								// when %a is undefined, %{!a} expands to %{!a}, but %!a expands to %a
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
						warn!("flags (! and ?) are not supported for %().");
					}
					let mut shellcmd = std::string::String::new();
					let req_pa = pa - 1;
					for ch in chars.by_ref() {
						chk_ps!(ch);
						if pa == req_pa {
							return match Command::new("sh").arg("-c").arg(&shellcmd).output() {
								Ok(out) => {
									if out.status.success() {
										Ok(core::str::from_utf8(&out.stdout)?.trim_end_matches('\n').into())
									} else {
										Err(eyre!("Shell expansion command did not succeed")
											.note(out.status.code().map_or("No status code".into(), |c| format!("Status code: {c}")))
											.section(core::str::from_utf8(&out.stdout)?.to_string().header("Stdout:"))
											.section(core::str::from_utf8(&out.stderr)?.to_string().header("Stderr:")))
									}
								}
								Err(e) => Err(eyre!(e).wrap_err("Shell expansion failed").note(shellcmd)),
							};
						}
						shellcmd.push(ch);
					}
					return Err(eyre!("Unexpected end of shell expansion command: `%({shellcmd}`"));
				}
				'[' => {
					todo!("what does %[] mean? www")
				}
				_ if ch.is_ascii_alphanumeric() || ch == '_' => {
					content.push(ch);
					first = false;
				}
				_ => {
					back!(ch);
					break;
				}
			}
		}
		exit_chk!();
		let out = self._rp_macro(&content, chars);
		if notflag {
			if question {
				return Ok("".into());
			}
			// when %a is undefined, %{!a} expands to %{!a}, but %!a expands to %a
			return Ok(out.unwrap_or_else(|e| {
				debug!("_rp_macro: {e:#}");
				if content.is_empty() { format!("%{content}") } else { format!("%{{!{content}}}") }.into()
			}));
		}
		Ok(out.unwrap_or_default())
	}

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
