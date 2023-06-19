use crate::error::ParserError;
use color_eyre::{eyre::bail, eyre::eyre, Help, Result};
use lazy_static::lazy_static;
use regex::Regex;
use std::{
	collections::HashMap,
	fmt::Write,
	fs::File,
	io::{BufRead, BufReader, Read},
	process::Command,
};
use tracing::{debug, error, warn};

const INTERNAL_MACROS: &[&str] = &[
	"trace",
	"dump",
	"echo",
	"warn",
	"error",
	"define",
	"undefine",
	"global",
	"uncompress",
	"expand",
	"S",
	"P",
	"F",
];

//? https://rpm-software-management.github.io/rpm/manual/spec.html
const PREAMBLES: &[&str] = &[
	"Name",
	"Version",
	"Release",
	"Epoch",
	"License",
	"SourceLicense",
	"Group",
	"Summary",
	"URL",
	"BugURL",
	"ModularityLabel",
	"DistTag",
	"VCS",
	"Distribution",
	"Vendor",
	"Packager",
	"BuildRoot",
	"AutoReqProv",
	"AutoReq",
	"AutoProv",
	"Requires",
	"Provides",
	"Conflicts",
	"Obsoletes",
	"Recommends",
	"Suggests",
	"Supplements",
	"Enhances",
	"OrderWithRequires",
	"BuildRequires",
	"BuildConflicts",
	"ExcludeArch",
	"ExclusiveArch",
	"ExcludeOS",
	"ExclusiveOS",
	"BuildArch",
	"BuildArchitectures",
	"Prefixes",
	"Prefix",
	"DocDir",
	"RemovePathPostfixes",
	// list
	"Source#",
	"Patch#",
];

#[derive(Default, Clone, Copy)]
enum PkgQCond {
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

#[derive(Clone, Default)]
struct Package {
	name: String,
	version: Option<String>,
	release: Option<String>,
	epoch: Option<u32>,
	condition: PkgQCond,
}
lazy_static! {
	static ref RE_PKGQCOND: Regex =
		Regex::new(r"\s+(>=?|<=?|=)\s+(\d+:)?([\w\d.^~]+)-([\w\d.^~]+)(.*)").unwrap();
}

const PKGNAMECHARSET: &str = "_-";

impl Package {
	fn new(name: String) -> Self {
		let mut x = Self::default();
		x.name = name;
		x
	}
	// Simple query: query without the <= and >= and versions and stuff. Only names.
	fn add_simple_query(pkgs: &mut Vec<Self>, query: &str) -> Result<()> {
		let query = query.trim();
		let mut last = String::new();
		for ch in query.chars() {
			if ch != ' ' && ch != ',' {
				if !(ch.is_alphanumeric() || PKGNAMECHARSET.contains(ch)) {
					return Err(eyre!("Invalid character `{ch}` found in package query.")
						.note(format!("query: `{query}`")));
				}
				last.write_char(ch)?;
			} else {
				pkgs.push(Package::new(std::mem::take(&mut last)));
			}
		}
		if !last.is_empty() {
			pkgs.push(Package::new(last));
		}
		Ok(())
	}
	fn add_query(pkgs: &mut Vec<Self>, query: &str) -> Result<()> {
		let query = query.trim(); // just in case
		if let Some((name, rest)) =
			query.split_once(|c: char| !c.is_alphanumeric() && !PKGNAMECHARSET.contains(c))
		{
			// the part that matches the good name is `name`. Check the rest.
			let mut pkg = Package::new(name.to_string());
			if let Some(caps) = RE_PKGQCOND.captures(rest) {
				pkg.condition = caps[1].into();
				if let Some(epoch) = caps.get(2) {
					let epoch =
						epoch.as_str().strip_suffix(':').expect("epoch no `:` by RE_PKGQCOND");
					pkg.epoch = Some(epoch.parse().map_err(|e| {
						eyre!("Cannot parse epoch to u32: `{epoch}`")
							.with_error(|| e)
							.suggestion("Epoch can only be positive integers")
					})?);
				}
				pkg.version = Some(caps[3].to_string());
				pkg.release = Some(caps[4].to_string());
				pkgs.push(pkg);
				if let Some(rest) = caps.get(5) {
					return Self::add_query(
						pkgs,
						rest.as_str().trim_start_matches(|c| " ,".contains(c)),
					);
				}
				Ok(())
			} else {
				Self::add_query(pkgs, rest.trim_start_matches(|c| " ,".contains(c)))
			}
		} else {
			// check if query matches pkg name
			if query.chars().any(|c| !c.is_alphanumeric() && !PKGNAMECHARSET.contains(c)) {
				return Err(eyre!("Invalid package name `{query}`")
					.suggestion("Use only alphanumerics, underscores and dashes."));
			}
			pkgs.push(Self::new(query.to_string()));
			Ok(())
		}
	}
}

#[derive(Default)]
struct RPMRequires {
	none: Vec<Package>,
	pre: Vec<Package>,
	post: Vec<Package>,
	preun: Vec<Package>,
	postun: Vec<Package>,
	pretrans: Vec<Package>,
	posttrans: Vec<Package>,
	verify: Vec<Package>,
	interp: Vec<Package>,
	meta: Vec<Package>,
}
impl RPMRequires {
	fn new() -> Self {
		Self::default()
	}
}

#[derive(Default)]
struct Scriptlets {
	pre: Option<String>,
	post: Option<String>,
	preun: Option<String>,
	postun: Option<String>,
	pretrans: Option<String>,
	posttrans: Option<String>,
	verify: Option<String>,

	triggerprein: Option<String>,
	triggerin: Option<String>,
	triggerun: Option<String>,
	triggerpostun: Option<String>,

	filetriggerin: Option<String>,
	filetriggerun: Option<String>,
	filetriggerpostun: Option<String>,
	transfiletriggerin: Option<String>,
	transfiletriggerun: Option<String>,
	transfiletriggerpostun: Option<String>,
}
impl Scriptlets {
	fn new() -> Self {
		Self::default()
	}
}

enum ConfigFileMod {
	None,
	MissingOK,
	NoReplace,
}

enum VerifyFileMod {
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

#[derive(Default)]
struct Files {
	// %artifact
	artifact: Vec<String>,
	// %ghost
	ghost: Vec<String>,
	// %config
	config: HashMap<String, ConfigFileMod>,
	// %dir
	dir: Vec<String>,
	// %readme (obsolete) = %doc
	// %doc
	doc: Vec<String>,
	// %license
	license: Vec<String>,
	// %verify
	verify: HashMap<String, VerifyFileMod>,
}
impl Files {
	fn new() -> Self {
		Self::default()
	}
}

struct Changelog {
	date: String, // ! any other?
	version: Option<String>,
	maintainer: String,
	email: String,
	message: String,
}

#[derive(Default)]
struct RPMSpec {
	globals: HashMap<String, String>,
	defines: HashMap<String, String>,

	// %description
	description: Option<String>,
	// %prep
	prep: Option<String>,
	// %generate_buildrequires
	generate_buildrequires: Option<String>,
	// %conf
	conf: Option<String>,
	// %build
	build: Option<String>,
	// %install
	install: Option<String>,
	// %check
	check: Option<String>,

	scriptlets: Scriptlets,
	files: Files,              // %files
	changelog: Vec<Changelog>, // %changelog

	//* preamble
	name: Option<String>,
	version: Option<String>,
	release: Option<String>,
	epoch: Option<i32>,
	license: Option<String>,
	sourcelicense: Option<String>,
	group: Option<String>,
	summary: Option<String>,
	sources: HashMap<i16, String>,
	patches: HashMap<i16, String>,
	// TODO icon
	// TODO nosource nopatch
	url: Option<String>,
	bugurl: Option<String>,
	modularitylabel: Option<String>,
	disttag: Option<String>,
	vcs: Option<String>,
	distribution: Option<String>,
	vendor: Option<String>,
	packager: Option<String>,
	// TODO buildroot
	autoreqprov: bool,
	autoreq: bool,
	autoprov: bool,
	requires: RPMRequires,
	provides: Vec<Package>,
	conflicts: Vec<Package>,
	obsoletes: Vec<Package>,
	recommends: Vec<Package>,
	suggests: Vec<Package>,
	supplements: Vec<Package>,
	enhances: Vec<Package>,
	orderwithrequires: Vec<Package>,
	buildrequires: Vec<Package>,
	buildconflicts: Vec<Package>,
	excludearch: Vec<String>,
	exclusivearch: Vec<String>,
	excludeos: Vec<String>,
	exclusiveos: Vec<String>,
	buildarch: Vec<String>, // BuildArchitectures BuildArch
	prefix: Option<String>, // Prefixes Prefix
	docdir: Option<String>,
	removepathpostfixes: Vec<String>,
}

impl RPMSpec {
	fn new() -> Self {
		Self {
			// buildroot
			autoreqprov: true,
			autoreq: true,
			autoprov: true,
			..Self::default()
		}
	}
}

/// A consumer that yields chars from a mutable String.
/// It is a bit more efficient if characters need to be
/// added into the String for the `.next()` iterations.
/// # Implementation
/// `Consumer` internally has `self.s` (String) storing
/// the output of the `BufReader` temporarily. However,
/// it is actually reversed. This is because operations
/// like `pop()` and `push()` are faster (`O(1)`) while
/// `remove(0)` and `insert(0, ?)` are slower (`O(n)`).
struct Consumer<R: std::io::Read = stringreader::StringReader<'static>> {
	s: String,
	r: Option<BufReader<R>>,
}

impl<R: std::io::Read> Consumer<R> {
	fn new(s: String, r: Option<BufReader<R>>) -> Self {
		Self { s: s.chars().rev().collect(), r }
	}
	fn push<'a>(&mut self, c: char) {
		self.s.push(c)
	}
	fn len(&self) -> usize {
		self.s.len()
	}
}

impl<R: std::io::Read> Iterator for Consumer<R> {
	type Item = char;

	fn next(&mut self) -> Option<Self::Item> {
		if let Some(c) = self.s.pop() {
			return Some(c);
		}
		if let Some(ref mut r) = self.r {
			let mut buf = [0; 64];
			if r.read(&mut buf).ok()? == 0 {
				None // EOF
			} else {
				self.s = match String::from_utf8(buf.into()) {
					Ok(s) => s.chars().rev().collect(),
					Err(e) => {
						error!("cannot parse buffer `{buf:?}`: {e}");
						return None;
					}
				};
				Some(unsafe { self.s.pop().unwrap_unchecked() })
			}
		} else {
			None
		}
	}
}

impl<R: std::io::Read> From<&str> for Consumer<R> {
	fn from(value: &str) -> Self {
		Consumer { s: value.chars().rev().collect(), r: None }
	}
}

enum ReadRawMacro {
	Parameter(String),
	Text(String),
}

// Process on required ... knackly?
#[derive(Debug)]
enum Pork<T = String> {
	Raw(String), // do you like raw meat
	Done(T),     // or well-done meat?
}

#[derive(Debug)]
enum Macro {
	Text(String),
	Lua(String),
	Par(String),
	Internal,
}

#[derive(Default)]
struct SpecParser {
	rpm: RPMSpec,
	errors: Vec<Result<(), ParserError>>,
	macros: HashMap<String, Pork<Macro>>,
}

impl SpecParser {
	fn parse_multiline(&self, sline: &str) {
		todo!();
	}
	fn parse_macro(&self, sline: &str) -> String {
		// run rpm --eval
		unimplemented!()
	}

	// returns true if it passes the check
	fn preamble_check(&mut self, name: String, ln: usize) -> bool {
		if !PREAMBLES.contains(&name.as_str()) {
			self.errors.push(Err(ParserError::UnknownPreamble(ln, name)));
			return false;
		}
		true
	}
	// todo BuildRequires?
	fn parse_requires(&mut self, sline: &str, ln: usize) -> bool {
		lazy_static! {
			static ref RE1: Regex =
				Regex::new(r"(?m)^Requires(?:\(([\w,\s]+)\))?:\s*(.+)$").unwrap();
			static ref RE2: Regex =
				Regex::new(r"(?m)([\w-]+)(?:\s*([>=<]{1,2})\s*([\d._~^]+))?").unwrap();
		}
		if let Some(caps) = RE1.captures(sline) {
			let spkgs = caps[caps.len()].trim();
			let mut pkgs = vec![];
			Package::add_query(&mut pkgs, spkgs).unwrap(); // fixme
			let modifiers = if caps.len() == 2 { &caps[2] } else { "none" };
			for modifier in modifiers.split(',') {
				let modifier = modifier.trim();
				let pkgs = pkgs.to_vec();
				match modifier {
					"none" => self.rpm.requires.none.extend(pkgs),
					"pre" => self.rpm.requires.pre.extend(pkgs),
					"post" => self.rpm.requires.post.extend(pkgs),
					"preun" => self.rpm.requires.preun.extend(pkgs),
					"postun" => self.rpm.requires.postun.extend(pkgs),
					"pretrans" => self.rpm.requires.pretrans.extend(pkgs),
					"posttrans" => self.rpm.requires.posttrans.extend(pkgs),
					"verify" => self.rpm.requires.verify.extend(pkgs),
					"interp" => self.rpm.requires.interp.extend(pkgs),
					"meta" => self.rpm.requires.meta.extend(pkgs),
					// _ => bail!("Unknown Modifier '{}' for Requires", modifier),
					_ => {
						self.errors
							.push(Err(ParserError::UnknownModifier(ln, modifier.to_string())));
					}
				}
			}
			return true;
		}
		false
	}
	fn arch() -> Result<String> {
		let s = String::from_utf8(Command::new("uname").arg("-m").output()?.stdout)?;
		Ok(s[..s.len() - 1].to_string()) // remove new line
	}
	// not sure where I've seen the docs, but there was one lying around saying you can define multiple
	// macros with the same name, and when you undefine it the old one recovers (stack?). I don't think
	// it is a good idea to do it like that (it is simply ridiculous and inefficient) but you can try
	fn load_macros(&mut self) -> Result<()> {
		// run rpm --showrc | grep "^Macro path"
		let binding = String::from_utf8(
			Command::new("sh")
				.args(["-c", "rpm --showrc|grep '^Macro path'|sed 's/Macro path: //'"])
				.output()?
				.stdout,
		)?;
		let paths = binding.trim().split(':');

		let re = Regex::new(r"(?m)^%([\w()]+)[\t ]+((\\\n|[^\n])+)$").unwrap();
		for path in paths {
			let path = path.replace("%{_target}", Self::arch()?.as_str());
			debug!(": {path}");
			for path in glob::glob(path.as_str())? {
				let path = path?;
				debug!("{}", path.display());
				let mut buf = vec![];
				let bytes = BufReader::new(File::open(&path)?).read_to_end(&mut buf)?;
				assert_ne!(bytes, 0, "Empty macro definition file '{}'", path.display());
				for cap in re.captures_iter(std::str::from_utf8(&buf)?) {
					if self.macros.contains_key(&cap[1]) {
						debug!(
							"Macro Definition duplicated: {} : '{:?}' | '{}'",
							&cap[1],
							self.macros.get(&cap[1]).unwrap(),
							&cap[2]
						);
						continue; // FIXME?
					}
					let name = &cap[1];
					if name.ends_with("()") {
						let mut content = cap[2].to_string();
						content.push(' '); // yup, we mark it using a space.
						self.macros.insert(
							unsafe { name.strip_suffix("()").unwrap_unchecked() }.to_string(),
							Pork::Raw(content),
						);
					}
					// we trim() just in case
					self.macros.insert(cap[1].to_string(), Pork::Raw(cap[2].trim().to_string()));
				}
			}
		}
		Ok(())
	}
	fn parse<R: std::io::Read>(&mut self, bufread: BufReader<R>) -> Result<()> {
		let re_preamble = Regex::new(r"(\w+):\s*(.+)").unwrap();
		let re_dnl = Regex::new(r"^%dnl\b").unwrap();
		let re_digit = Regex::new(r"\d+$").unwrap();
		let mut preambles: HashMap<String, Vec<String>> = HashMap::new();
		let mut list_preambles: HashMap<String, HashMap<i16, String>> = HashMap::new();
		'll: for (line_number, line) in bufread.lines().enumerate() {
			let line = line?;
			let sline = line.trim();
			// todo
			// * we have to parse %macros here (just like rpm)
			if sline.is_empty() || sline.starts_with('#') || re_dnl.is_match(sline) {
				continue;
			}
			// Check for Requires special preamble syntax first
			if self.parse_requires(sline, line_number) {
				continue;
			}
			// only then do we check for other preambles
			for cap in re_preamble.captures_iter(sline) {
				// check for list_preambles
				if let Some(digitcap) = re_digit.captures(&cap[1]) {
					let sdigit = &digitcap[0];
					let digit: i16 = sdigit.parse()?;
					let name = &cap[1][..cap[1].len() - sdigit.len()];
					let sname = name.to_string();
					if !self.preamble_check(format!("{name}#"), line_number) {
						continue 'll;
					}
					self.add_list_preamble(name, digit, cap[2].to_string())?;
				} else {
					let name = cap[1].to_string();
					if !self.preamble_check(name, line_number) {
						continue 'll;
					}
					self.add_preamble(&cap[1], cap[2].to_string(), line_number)?;
				}
			}
			// ! error?
		}
		if !self.errors.is_empty() {
			return Err(eyre!("{:#?}", self.errors));
		}
		Ok(())
	}

	fn add_list_preamble(&mut self, name: &str, digit: i16, value: String) -> Result<()> {
		let value = value.to_owned();
		let rpm = &mut self.rpm;
		match name {
			"Source" => rpm.sources = todo!(),
			"Patch" => rpm.patches = todo!(),
			_ => bail!("BUG: failed to match preamble '{name}'"),
		}
		Ok(())
	}

	// TODO (wip) call this on the spot? or else macros can't be parsed correctly
	fn add_preamble(&mut self, name: &str, value: String, ln: usize) -> Result<()> {
		let rpm = &mut self.rpm;

		macro_rules! opt {
			($x:ident $y:ident) => {
				if name == stringify!($x) {
					if let Some(ref old) = rpm.$y {
						warn!(
							"overriding existing {} preamble value `{old}` to `{value}`",
							stringify!($x)
						);
						self.errors
							.push(Err(ParserError::Duplicate(ln, stringify!($x).to_string())));
					}
					rpm.name = Some(value);
					return Ok(());
				}
			};
			(~$x:ident $y:ident) => {
				if name == stringify!($x) {
					rpm.name = Some(value.parse()?);
					return Ok(());
				}
			};
		}

		opt!(Name name);
		opt!(Version version);
		opt!(Release release);
		opt!(License license);
		opt!(SourceLicense sourcelicense);
		opt!(Group group);
		opt!(Summary summary);
		opt!(URL url);
		opt!(BugURL bugurl);
		opt!(ModularityLabel modularitylabel);
		opt!(DistTag disttag);
		opt!(VCS vcs);
		opt!(Distribution distribution);
		opt!(Vendor vendor);
		opt!(Packager packager);
		opt!(~AutoReqProv autoreqprov);
		opt!(~AutoReq autoreq);
		opt!(~AutoProv autoprov);

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
			"OrderWithRequires" => {}
			"BuildRequires" => Package::add_query(&mut rpm.buildrequires, &value)?,
			"BuildConflicts" => {}
			"ExcludeArch" => {}
			"ExclusiveArch" => {}
			"ExcludeOS" => {}
			"ExclusiveOS" => {}
			"BuildArch" => {}
			"BuildArchitectures" => {}
			"Prefixes" => {}
			"Prefix" => {}
			"DocDir" => {}
			"RemovePathPostfixes" => {}
			_ => bail!("BUG: failed to match preamble '{name}'"),
		}
		Ok(())
	}

	fn _internal_macro(&mut self, name: &str) -> Result<String> {
		todo!()
	}

	// design issue: we kinda need to know if it's Macro::Par before we can grab the args...?
	fn _rp_macro(&mut self, name: &str, reader: &mut Consumer) -> Option<String> {
		debug!("getting %{name}");
		let m = self.macros.get(name)?;
		match m {
			Pork::Done(Macro::Text(val)) => Some(val.into()),
			Pork::Done(Macro::Internal) => self._internal_macro(name).ok(),
			Pork::Done(Macro::Par(ph)) => {
				todo!() // parameterized macro
			}
			Pork::Done(Macro::Lua(code)) => {
				todo!() // we have this in `rpmio/rpmlua.rs` already. kinda?
			}
			Pork::Raw(def) => {
				// it's fucking raw!!
				let def = def.to_string();
				if def.ends_with(' ') {
					// parameterized macro
					todo!() // another parser? dunno
				}
				let m = self._parse_simple_macro_definition(&def).unwrap();
				self.macros.insert(name.to_string(), m);
				self._rp_macro(name, reader)
			}
		}
	}

	// used by _rp_macro, expands macro definition from Pork::Raw.
	// recursive! and doesn't parse parameterized macros
	fn _parse_simple_macro_definition(&mut self, definition: &str) -> Result<Pork<Macro>> {
		// first of all it's not internal (filled in Self::new())
		let mut res = String::new();
		let mut percent = false;
		let mut chars: Consumer = Consumer::from(definition);
		while let Some(ch) = chars.next() {
			if ch == '%' {
				if percent {
					res += "%";
					percent = false; // %%%% will be parsed correctly in this way
					continue;
				}
				percent = true;
				continue;
			}
			if percent {
				chars.push(ch);
				res += &self._read_raw_macro_use(&mut chars)?;
				percent = false;
			} else {
				res.write_char(ch).unwrap();
			}
		}
		Ok(Pork::Done(Macro::Text(res)))
	}

	/// parse the stuff after %, and determines `{[()]}`. returns expanded macro.
	/// Assumption: `chars` is a str Consumer (no reader).
	/// FIXME please REFACTOR me!!
	fn _read_raw_macro_use(&mut self, chars: &mut Consumer) -> Result<String> {
		debug!("reading macro");
		let mut notflag = false;
		let mut question = false;
		let mut content = String::new();
		let l = chars.len();
		macro_rules! flagmacrohdl {
			($name:expr, $consumer:expr) => {
				let out = self._rp_macro($name, &mut Consumer::from($consumer));
				if notflag {
					if question {
						return Ok("".into());
					}
					// when %a is undefined, %{!a} expands to %{!a}, but %!a expands to %a
					if content.is_empty() {
						return Ok(out.unwrap_or_else(|| format!("%{}", $name)));
					} else {
						return Ok(out.unwrap_or_else(|| format!("%{{!{}}}", $name)));
					}
				}
				if question {
					return Ok(out.unwrap_or_default());
				}
				return out.ok_or(eyre!("Macro not found: %{content}"));
			};
			($name:expr) => {
				flagmacrohdl!($name, "")
			};
		}
		macro_rules! _q {
			() => {{
				if question {
					warn!("Seeing double `?` flag in macro use. Ignoring.");
				}
				question = true;
				continue;
			}};
		}
		while let Some(ch) = chars.next() {
			// we read until we encounter '}' or ':' or the end
			match ch {
				'!' => notflag = !notflag,
				'?' => _q!(),
				'{' => {
					if chars.len() + 1 != l {
						chars.push('{');
						break;
					}
					let mut name = String::new();
					while let Some(ch) = chars.next() {
						if ch == '}' {
							flagmacrohdl!(&name);
						}
						if ch == ':' {
							let mut content = String::new();
							for ch in chars.by_ref() {
								if ch == '}' {
									if question {
										if self.macros.contains_key(&name) ^ notflag {
											return Ok("".to_string());
										} else {
											// expand content?
											return Ok(self.parse_macro(&content));
										}
									}
									flagmacrohdl!(&name, &*content);
								}
								content.write_char(ch)?;
							}
						}
						if ch == '?' {
							_q!();
						}
						if ch == '!' {
							notflag = !notflag;
							continue;
						}
						name.write_char(ch)?;
					}
				}
				'(' => {
					if !content.is_empty() {
						chars.push('(');
						break;
					}
					if notflag || question {
						warn!("flags (! and ?) are not supported for %().");
					}
					let mut shellcmd = String::new();
					for ch in chars.by_ref() {
						if ch == ')' {
							return match Command::new("sh").arg("-c").arg(shellcmd).output() {
								Ok(out) => {
									Ok(String::from_utf8(out.stdout)?.trim_end_matches('\n').into())
								}
								Err(e) => Err(eyre!(e)),
							};
						}
						shellcmd.write_char(ch)?;
					}
					return Err(eyre!("Unexpected end of shell command, for `%({shellcmd}`"));
				}
				'[' => {
					todo!("what does %[] mean? www")
				}
				_ => {
					if !(ch.is_alphanumeric() || ch == '_') {
						chars.push(ch);
						break;
					}
					content.write_char(ch)?;
				}
			}
		}
		flagmacrohdl!(&content);
	}

	fn new() -> Self {
		let mut obj = Self { rpm: RPMSpec::new(), errors: vec![], macros: HashMap::new() };
		INTERNAL_MACROS.iter().for_each(|name| {
			obj.macros.insert(name.to_string(), Pork::Done(Macro::Internal));
		});
		obj
	}
}

fn _single(value: &Vec<String>) -> &String {
	assert_eq!(value.len(), 1);
	&value[0]
}
fn _ssin(value: &Vec<String>) -> Option<String> {
	Some(_single(value).to_owned())
}
fn _sbin(value: &Vec<String>) -> Result<bool> {
	Ok(_single(value).to_owned().parse()?)
}

mod tests {
	use super::*;
	use std::fs::File;

	#[test]
	fn parse_spec() -> Result<()> {
		let f = File::open("../tests/test.spec")?;
		let f = BufReader::new(f);

		let mut sp = SpecParser::new();
		sp.parse(f)?;
		println!("{}", sp.rpm.name.unwrap_or_default());
		println!("{}", sp.rpm.summary.unwrap_or_default());
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
		parser.macros.insert("macrohai".to_string(), Pork::Raw("hai hai".to_string()));
		assert_eq!(parser._read_raw_macro_use(&mut ("macrohai".into()))?, "hai hai");
		Ok(())
	}
	#[test]
	fn text_recursive_macro_expand() -> Result<()> {
		let mut parser = super::SpecParser::new();
		parser.macros.insert("mhai".to_string(), Pork::Raw("hai hai".to_string()));
		parser.macros.insert("quadhai".to_string(), Pork::Raw("%mhai %{mhai}".to_string()));
		assert_eq!(parser._read_raw_macro_use(&mut ("quadhai".into()))?, "hai hai hai hai");
		Ok(())
	}
	#[test]
	fn text_quoting_recursive_macro_expand() -> Result<()> {
		let mut parser = super::SpecParser::new();
		parser.macros.insert("mhai".to_string(), Pork::Raw("hai hai".to_string()));
		parser.macros.insert("idk".to_string(), Pork::Raw("%!?mhai %?!mhai %{mhai}".to_string()));
		parser
			.macros
			.insert("idk2".into(), Pork::Raw("%{?mhai} %{!mhai} %{!?mhai} %{?!mhai}".into()));
		parser.macros.insert("aaa".to_string(), Pork::Raw("%idk %idk2".to_string()));
		assert_eq!(parser._read_raw_macro_use(&mut ("aaa".into()))?, "  hai hai hai hai hai hai  ");
		Ok(())
	}
	#[test]
	fn shell_macro_expand() -> Result<()> {
		let mut parser = super::SpecParser::new();
		parser
			.macros
			.insert("x".to_string(), Pork::Raw("%(echo haai | sed 's/a/aa/g')".to_string()));
		assert_eq!(parser._read_raw_macro_use(&mut ("x".into()))?, "haaaai");
		Ok(())
	}
}
