use crate::error::ParserError;
use color_eyre::{eyre::bail, eyre::eyre, Result};
use lazy_static::lazy_static;
use regex::Regex;
use std::{
	collections::HashMap,
	fmt::Write,
	fs::File,
	io::{BufRead, BufReader, Read},
	process::Command,
	str::Chars,
};
use tracing::{debug, warn};

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

#[derive(Clone, Default)]
struct Package {
	name: String,
	version: Option<String>,
	release: Option<String>,
	epoch: Option<i32>,
	condition: Option<String>,
}
impl Package {
	fn new(name: String) -> Self {
		let mut x = Self::default();
		x.name = name;
		x
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
	// %doc
	doc: Vec<String>,
	// %license
	license: Vec<String>,
	// %readme (obsolete)
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
	// icon
	// nosource nopatch
	url: Option<String>,
	bugurl: Option<String>,
	modularitylabel: Option<String>,
	disttag: Option<String>,
	vsc: Option<String>,
	distribution: Option<String>,
	vendor: Option<String>,
	packager: Option<String>,
	// buildroot
	autoreqprov: bool,
	autoreq: bool,
	autoprov: bool,
	requires: RPMRequires,
	provides: Vec<Package>,
	conflicts: Vec<Package>,
	obsoletes: Vec<Package>,
	suggests: Vec<Package>,
	// recommends suggests supplements enhances
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
struct Consumer<R: std::io::Read> {
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
}

impl<R: std::io::Read> Iterator for Consumer<R> {
	type Item = char;

	fn next(&mut self) -> Option<Self::Item> {
		if let Some(c) = self.s.pop() {
			return Some(c);
		}
		if let Some(r) = self.r {
			let mut buf = [0; 64];
			if r.read(&mut buf).ok()? == 0 {
				None // EOF
			} else {
				self.s = match String::from_utf8(buf.into()) {
					Ok(s) => s.chars().rev().collect(),
					Err(e) => {
						eyre!("cannot parse buffer `{buf:?}`: {e}");
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
	fn parse_macro(&self, sline: &str) {
		// run rpm --eval
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
			let spkgs = &caps[caps.len()].trim();
			let mut pkgs = vec![];
			for cpkg in RE2.captures_iter(spkgs) {
				let mut pkg = Package::new(cpkg[cpkg.len() - 1].to_string());
				if cpkg.len() == 3 {
					// get rid of spaces I guess
					pkg.condition = Some(format!("{}{}", &cpkg[1], &cpkg[2]));
				}
				pkgs.push(pkg);
			}
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
						let content = cap[2].to_string();
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
		let re = Regex::new(r"(\w+):\s*(.+)").unwrap();
		let re_dnl = Regex::new(r"^%dnl\b").unwrap();
		let re_digit = Regex::new(r"\d+$").unwrap();
		let mut preambles: HashMap<String, Vec<String>> = HashMap::new();
		let mut list_preambles: HashMap<String, HashMap<i16, String>> = HashMap::new();
		'll: for (line_number, line) in bufread.lines().enumerate() {
			let line = line?;
			let sline = line.trim();
			// * we have to parse %macros here (just like rpm)
			if sline.is_empty() || sline.starts_with('#') || re_dnl.is_match(sline) {
				continue;
			}
			// Check for Requires special preamble syntax first
			if self.parse_requires(sline, line_number) {
				continue;
			}
			// only then do we check for other preambles
			for cap in re.captures_iter(sline) {
				// key already exists
				if preambles.contains_key(&cap[1]) {
					if re_digit.is_match(&cap[1]) {
						self.errors
							.push(Err(ParserError::Duplicate(line_number, cap[1].to_string())));
						continue 'll;
					}
					preambles.get_mut(&cap[1]).unwrap().push(cap[2].to_string());
					continue 'll;
				}
				// check for list_preambles
				if let Some(digitcap) = re_digit.captures(&cap[1]) {
					let sdigit = &digitcap[0];
					let digit: i16 = sdigit.parse()?;
					let name = &cap[1][..cap[1].len() - sdigit.len()];
					let sname = name.to_string();
					if !self.preamble_check(format!("{}#", name), line_number) {
						continue 'll;
					}
					if !list_preambles.contains_key(&sname) {
						list_preambles.insert(name.to_string(), HashMap::new());
					}
					match &mut list_preambles.get_mut(&sname) {
						Some(hm) => hm.insert(digit, cap[2].to_string()),
						None => bail!("BUG: added HashMap gone"),
					};
				} else {
					let name = cap[1].to_string();
					if !self.preamble_check(name, line_number) {
						continue 'll;
					}
					// create key with new value (normal preambles)
					preambles.insert(cap[1].to_string(), vec![cap[2].to_string()]);
				}
			}
			// ! error?
		}
		preambles.iter().map(|(k, v)| self.set_preamble(k, v)).collect::<Result<Vec<_>>>()?;
		list_preambles
			.iter()
			.map(|(k, v)| self.set_list_preamble(k, v))
			.collect::<Result<Vec<_>>>()?;
		if !self.errors.is_empty() {
			return Err(eyre!("{:#?}", self.errors));
		}
		Ok(())
	}

	fn set_list_preamble(&mut self, name: &str, value: &HashMap<i16, String>) -> Result<()> {
		let value = value.to_owned();
		let rpm = &mut self.rpm;
		match name {
			"Source" => rpm.sources = value,
			"Patch" => rpm.patches = value,
			_ => bail!("BUG: failed to match preamble '{}'", name),
		}
		Ok(())
	}

	fn set_preamble(&mut self, name: &String, value: &Vec<String>) -> Result<()> {
		let rpm = &mut self.rpm;
		match name.as_str() {
			"Name" => rpm.name = _ssin(value),
			"Version" => rpm.version = _ssin(value),
			"Release" => rpm.release = _ssin(value),
			"Epoch" => {
				rpm.epoch = _ssin(value).map(|x| x.parse().expect("Failed to decode epoch to int"))
			}
			"License" => rpm.license = _ssin(value),
			"SourceLicense" => rpm.sourcelicense = _ssin(value),
			"Group" => rpm.group = _ssin(value), // ! confirm?
			"Summary" => rpm.summary = _ssin(value),
			"URL" => rpm.url = _ssin(value),
			"BugURL" => rpm.bugurl = _ssin(value),
			"ModularityLabel" => rpm.modularitylabel = _ssin(value),
			"DistTag" => rpm.disttag = _ssin(value),
			"VCS" => {}
			"Distribution" => rpm.distribution = _ssin(value),
			"Vendor" => rpm.vendor = _ssin(value),
			"Packager" => rpm.packager = _ssin(value),
			"AutoReqProv" => rpm.autoreqprov = _sbin(value)?,
			"AutoReq" => rpm.autoreq = _sbin(value)?,
			"AutoProv" => rpm.autoprov = _sbin(value)?,
			"Provides" => {}
			"Conflicts" => {}
			"Obsoletes" => {}
			"Recommends" => {}
			"Suggests" => {}
			"Supplements" => {}
			"Enhances" => {}
			"OrderWithRequires" => {}
			"BuildRequires" => {}
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

	fn _internal_macro(&mut self, name: &str) -> Result<&str> {
		todo!()
	}

	fn _rp_macro(&mut self, name: &str, args: &str) -> Option<&str> {
		let m = self.macros.get(name)?;
		match m {
			Pork::Done(Macro::Text(val)) => Some(val),
			Pork::Done(Macro::Internal) => self._internal_macro(name).ok(),
			Pork::Done(Macro::Par(ph)) => {
				todo!() // parameterized macro
			}
			Pork::Done(Macro::Lua(code)) => {
				todo!() // we have this in `rpmio/rpmlua.rs` already. kinda?
			}
			Pork::Raw(m) => {
				// it's fucking raw!!
				if m.ends_with(' ') {
					// parameterized macro
					todo!() // another parser? dunno
				}
				self._parse_macro_definition(name, m, args) // calls _rp_macro() again
			}
		}
	}

	// used by _rp_macro, expands macro definition from Pork::Raw.
	// recursive! and doesn't parse parameterized macros
	fn _parse_macro_definition(&mut self, name: &str, m: &String, args: &str) -> Option<&str> {
		// first of all it's not internal (filled in Self::new())
		let mut res = String::new();
		let mut percent = false;
		let mut is_substitution = false;
		let mut chars: Consumer<stringreader::StringReader> = Consumer::from(m.as_str()); // any type that impl Read works
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
				res += self._read_raw_macro_use(&mut chars).ok()?;
				percent = false;
			} else {
				res.write_char(ch);
			}
		}
		self.macros.insert(
			name.to_string(),
			Pork::Done(if is_substitution { Macro::Par(res) } else { Macro::Text(res) }),
		);
		return self._rp_macro(name, args); // FIXME we can probably optimise this part.
	}

	/// parse the stuff after %, and determines "{[(". returns expanded macro.
	fn _read_raw_macro_use<R: std::io::Read>(&mut self, chars: &mut Consumer<R>) -> Result<&str> {
		// read until we get name?
		let mut notflag = false;
		let mut question = false;
		let mut content = String::new();
		let mut add = "";
		while let Some(ch) = chars.next() {
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
					if !content.is_empty() {
						add = "{";
						break;
					}
					let mut name = String::new();
					while let Some(ch) = chars.next() {
						if ch == '}' {
							// TODO flags
							return self
								._rp_macro(&name, "")
								.ok_or(eyre!("Macro not found: %{name}"));
						}
						if ch == ':' {
							let mut content = String::new();
							while let Some(ch) = chars.next() {
								if ch == '}' {
									// TODO flags
									return self
										._rp_macro(&name, &content)
										.ok_or(eyre!("Macro not found: %{name}"));
								}
							}
						}
						name.write_char(ch);
					}
				}
				'(' => {
					if !content.is_empty() {
						add = "(";
						break;
					}
					if notflag || question {
						warn!("flags (! and ?) are not supported for %().");
					}
					let mut shellcmd = String::new();
					while let Some(ch) = chars.next() {
						if ch == ')' {
							return match Command::new("sh").arg("-c").arg(shellcmd).output() {
								Ok(out) => Ok(String::from_utf8(out.stdout)?.as_str()),
								Err(e) => Err(eyre!(e)),
							};
						}
						shellcmd.write_char(ch);
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
					content.write_char(ch);
				}
			}
		}
		// TODO flags
		let out = self._rp_macro(&content, "").map(|x| {
			let x = x.to_string();
			x.push_str(add);
			x.as_str()
		});
		if notflag {
			warn!("Found `%!...`, returning nothing.");
			return Ok("");
		}
		if question {
			return Ok(out.unwrap_or_default());
		}
		return out.ok_or(eyre!("Macro not found: %{content}"));
	}

	fn new() -> Self {
		let mut obj = Self { rpm: RPMSpec::new(), errors: vec![], macros: HashMap::new() };
		INTERNAL_MACROS
			.iter()
			.map(|name| obj.macros.insert(name.to_string(), Pork::Done(Macro::Internal)));
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
}
