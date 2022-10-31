use std::{collections::HashMap, fs, hash::Hash, path::Path, io::BufRead};

use regex::Regex;
use anyhow::Result;
use crate::error::ParserError;

struct Package {
    name: String,
    version: String,
    release: String,
    epoch: i32,
}

struct RPMRequires {
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

struct Changelog {
    date: String, // ! any other?
    version: Option<String>,
    maintainer: String,
    email: String,
    message: String,
}

const PREAMBLES: &[&str] = &[
    "Name",
    "Version",
    "Release",
    "Epoch",
    "License",
    "SourceLicense",
    "Group",
    "Summary",
    "Source#",
    "Patch#",
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
];

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
    files: Files,         // %files
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
    sources: Vec<String>,
    patches: Vec<String>,
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
    prefix: Option<String>,         // Prefixes Prefix
    docdir: Option<String>,
    removepathpostfixes: Vec<String>,
}

impl RPMRequires {
    fn new() -> Self {
        Self {
            interp: vec![],
            meta: vec![],
            post: vec![],
            posttrans: vec![],
            postun: vec![],
            pre: vec![],
            pretrans: vec![],
            preun: vec![],
            verify: vec![],
        }
    }
}

impl Scriptlets {
    fn new() -> Self {
        Self {
            pre: None,
            post: None,
            preun: None,
            postun: None,
            pretrans: None,
            posttrans: None,
            verify: None,
            triggerprein: None,
            triggerin: None,
            triggerun: None,
            triggerpostun: None,
            filetriggerin: None,
            filetriggerun: None,
            filetriggerpostun: None,
            transfiletriggerin: None,
            transfiletriggerun: None,
            transfiletriggerpostun: None,
        }
    }
}

impl Files {
    fn new() -> Self {
        Self {
            artifact: vec![],
            ghost: vec![],
            config: HashMap::new(),
            dir: vec![],
            doc: vec![],
            license: vec![],
            verify: HashMap::new()
        }
    }
}

impl RPMSpec {
    fn new() -> Self {
        Self {
            globals: HashMap::new(),
            defines: HashMap::new(),
            description: None,
            prep: None,
            generate_buildrequires: None,
            conf: None,
            build: None,
            install: None,
            check: None,
            scriptlets: Scriptlets::new(),
            files: Files::new(),
            changelog: vec![],
            name: None,
            version: None,
            release: None,
            epoch: None,
            license: None,
            sourcelicense: None,
            group: None,
            summary: None,
            sources: vec![],
            patches: vec![],
            // icon
            // nosource nopatch
            url: None,
            bugurl: None,
            modularitylabel: None,
            disttag: None,
            vsc: None,
            distribution: None,
            vendor: None,
            packager: None,
            // buildroot
            autoreqprov: true,
            autoreq: true,
            autoprov: true,
            requires: RPMRequires::new(),
            provides: vec![],
            conflicts: vec![],
            obsoletes: vec![],
            suggests: vec![],
            // recommends suggests supplements enhances
            orderwithrequires: vec![],
            buildrequires: vec![],
            buildconflicts: vec![],
            excludearch: vec![],
            exclusivearch: vec![],
            excludeos: vec![],
            exclusiveos: vec![],
            buildarch: vec![], // BuildArchitectures BuildArch
            prefix: None,         // Prefixes Prefix
            docdir: None,
            removepathpostfixes: vec![],
        }
    }
}

struct SpecParser<'a> { 
    rpm: RPMSpec,
    bufread: &'a mut dyn BufRead
}

impl SpecParser<'_> {
    fn parse(&mut self) -> Result<()> {
        let re = Regex::new(r"(\w+):\s*(.+)").unwrap();
        let mut preambles: HashMap<String, Vec<&str>> = HashMap::new();
        for line in self.bufread.lines() {
            let line = line?;
            if line.starts_with('#') || line.starts_with("%dnl ") {
                continue;
            }
            if line.starts_with('%') {

            }
            for cap in re.captures_iter(line.as_str()) {
                if preambles.contains_key(&cap[1]) {
                    preambles.get_mut(&cap[1]).unwrap().push(&cap[2]);
                }
            }
        }
        Ok(())
    }
    fn set_preamble<'b, 'a>(&mut self, name: &'a str, value: &'b str) -> Result<(), ParserError> {
        let rpm = &mut self.rpm;
        match name {
            "Name" => rpm.name = Some(value.into()),
            "Version" => {},
            "Release" => {},
            "Epoch" => {},
            "License" => {},
            "SourceLicense" => {},
            "Group" => {},
            "Summary" => {},
            "Source#" => {},
            "Patch#" => {},
            "URL" => {},
            "BugURL" => {},
            "ModularityLabel" => {},
            "DistTag" => {},
            "VCS" => {},
            "Distribution" => {},
            "Vendor" => {},
            "Packager" => {},
            "BuildRoot" => {},
            "AutoReqProv" => {},
            "AutoReq" => {},
            "AutoProv" => {},
            "Requires" => {},
            "Provides" => {},
            "Conflicts" => {},
            "Obsoletes" => {},
            "Recommends" => {},
            "Suggests" => {},
            "Supplements" => {},
            "Enhances" => {},
            "OrderWithRequires" => {},
            "BuildRequires" => {},
            "BuildConflicts" => {},
            "ExcludeArch" => {},
            "ExclusiveArch" => {},
            "ExcludeOS" => {},
            "ExclusiveOS" => {},
            "BuildArch" => {},
            "BuildArchitectures" => {},
            "Prefixes" => {},
            "Prefix" => {},
            "DocDir" => {},
            "RemovePathPostfixes" => {},
            _ => return Err(ParserError::UnknownPreamble(name, value))
        }
        Ok(())
    }
    fn parse_macros(line: &str) -> Result<()> {
        Ok(())
    }
}
impl From<&mut dyn BufRead> for SpecParser<'_> {
    fn from(f: &mut dyn BufRead) -> Self {
        Self {
            bufread: f,
            rpm: RPMSpec::new()
        }
    }
}
