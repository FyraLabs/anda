use pest::{Parser, iterators::Pair};
use pest_derive::Parser;
use std::collections::BTreeMap;

use serde::{Deserializer, Serializer};

// Special macros that need not be expanded
const SPECIAL_MACROS: &[&str] = &[
    "prep",
    "build",
    "install",
    "check",
    "clean",
    "pre",
    "post",
    "preun",
    "postun",
    "pretrans",
    "posttrans",
    "triggerin",
    "triggerun",
    "triggerpostun",
    "verifyscript",
    "filetriggerin",
    "filetriggerun",
    "filetriggerpostun",
    "define",
    "undefine",
    "global",
];
pub struct Spec {
    pub name: String,
    pub epoch: String,
    pub version: String,
    pub release: String,
    pub arch: String,
    pub summary: String,
    pub description: Option<String>,
    pub license: String,
    pub url: String,
    pub sources: Vec<String>,
    pub patches: Vec<String>,
    pub build_requires: Vec<String>,
    pub requires: Vec<String>,
    pub provides: Vec<String>,
    pub conflicts: Vec<String>,
    pub obsoletes: Vec<String>,
}

/// RPM Spec value
#[derive(Parser, Debug)]
#[grammar = "rpm.pest"]
pub struct SpecParser;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Body {
    pub macros: BTreeMap<String, String>,
    pub statements: BTreeMap<String, String>,
}

fn print_pairs(pair: Pair<Rule>) {
    // check how many levels deep the pair is
    let indent = pair.as_span().start_pos().pos();
    println!("Rule {:?} @ {}", pair.as_rule(), indent);
    println!("  : {}", pair.as_str());

    let rule_type = pair.as_rule();
    match rule_type {
        Rule::property => {
            let mut inner = pair.into_inner();
            let key = inner.next().unwrap().as_str();
            let value = inner.next().unwrap().as_str();
            println!("  - key: {}", key);
            println!("  - value: {}", value);
        }
        _ => {
            for inner_pair in pair.into_inner() {
                print_pairs(inner_pair);
            }
        }
    }
    // for inner_pair in pair.into_inner() {
    //     print_pairs(inner_pair);
    // }
}

impl Body {
    pub fn parse(spec: &str) {
        let spec = SpecParser::parse(Rule::file, spec).unwrap();
        for pair in spec {
            println!("Rule: {:?}", pair.as_rule());
            // recursively print the pairs

            for inner_pair in pair.into_inner() {
                print_pairs(inner_pair);
            }
        }
    }
}

// RPM Macro
pub struct Macro(String, String);

/// RPM Macro definition
pub struct MacroDef {}

pub struct Changelog {
    // parse from Ddd Mmm dd yyyy (Day of week, Month, Day, Year)
    pub date: chrono::NaiveDate,
    pub author: String,
    pub email: String,
    /// Version-release of the package
    pub version: String,
    pub changes: Vec<String>,
}

impl Changelog {
    pub fn parse() {
        // parse date

        let string = r#"* Fri Oct 21 2022 John Doe <packager@example.com> - 0.1.6-1.um37
- local build
* Sat Oct 22 2022 Cappy Ishihara <cappy@cappuchino.xyz>
- test
        "#;

        // separate changelog by lines
        let lines: Vec<&str> = string.lines().collect();

        // separate changelog entries by the line that starts with '*'
        let mut entries: Vec<Vec<&str>> = Vec::new();
        let entry: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut entry_lines: Vec<&str> = Vec::new();
        for line in lines {
            let mut data: (&str, &str) = ("", "");
            if line.starts_with('*') {
                data.0 = line;

                // get the following lines, if they start with '-'
                let mut next_lines: Vec<&str> = Vec::new();
            }
            entry_lines.push(line);
        }
        println!("{:?}", entries);

        let date = chrono::NaiveDate::parse_from_str("Fri Mar 15 2019", "%a %b %d %Y");
        println!("{:?}", date);
    }
}

#[test]
fn test_sadas() {
    Changelog::parse();

    let specfile = include_str!("../../tests/umpkg.spec");
    let spec = SpecParser::parse(Rule::file, specfile).unwrap();
    println!("{:?}", spec);
    let spec = Body::parse(specfile);
}
