use pest::{iterators::Pair, Parser};
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

#[derive(Debug)]
pub struct Changelog {
    // parse from Ddd Mmm dd yyyy (Day of week, Month, Day, Year)
    pub date: chrono::NaiveDate,
    pub author: String,
    /// Version-release of the package
    pub version: Option<String>,
    pub changes: Vec<String>,
}

impl Changelog {
    pub fn parse() {
        // parse date

        let string = r#"* Fri Oct 21 2022 John Doe <packager@example.com> - 0.1.6-1.um37
- local build
- among us
* Sat Oct 22 2022 Cappy Ishihara <cappy@cappuchino.xyz>
- test
"#
        .trim_start();

        // split by *
        let change = string.split('*').collect::<Vec<&str>>();

        // variable box so we can redefine it later

        for c in change {
            let mut chdate = Box::new(chrono::NaiveDate::from_ymd(1970, 1, 1));
            let mut chauthor = String::new();
            let mut chversion = None;
            let mut chchanges = Vec::new();

            // if the line is empty, skip it
            if c.trim().is_empty() {
                continue;
            }

            // parse the first line
            let mut lines = c.lines();
            if let Some(line) = lines.next() {
                let line = line.trim_start();
                println!("line: {}", line);

                // parse date

                // split by the 3rd space (%a %b %d %Y <Author> - <Version>)
                let split = line.split_whitespace().collect::<Vec<&str>>();
                let spl = split.split_at(4);
                let date = spl.0.join(" ");
                // let date = split.next().unwrap();
                // println!("date: {}", date);

                let date = chrono::NaiveDate::parse_from_str(&date, "%a %b %d %Y");
                // println!("date: {:?}", date);

                chdate = Box::new(date.unwrap());

                // parse author
                let joined = spl.1.join(" ");

                let split2 = joined.split_once(" - ");

                let author = {
                    if let Some(split2) = split2 {
                        split2.0.to_string()
                    } else {
                        joined.clone()
                    }
                };

                chauthor = author;

                let version = { split2.map(|split2| split2.1.to_string()) };

                chversion = version
            }

            // parse the rest of the lines that start with -
            for line in lines {
                let line = line.trim_start();
                if line.starts_with('-') {
                    chchanges.push(line.strip_prefix('-').unwrap().trim_start().to_string());
                }
            }
            /* println!("chdate: {:?}", chdate);
            println!("chauthor: {}", chauthor);
            println!("chversion: {:?}", chversion);
            println!("chchanges: {:?}", chchanges); */

            let changelog = Changelog {
                date: *chdate,
                author: chauthor,
                version: chversion,
                changes: chchanges,
            };

            println!("changelog: {:#?}", changelog);
        }

        // println!("change: {:?}", change);
    }
}

#[test]
fn test_sadas() {
    Changelog::parse();

    let specfile = include_str!("../../tests/umpkg.spec");
    let spec = SpecParser::parse(Rule::file, specfile).unwrap();
    // println!("{:?}", spec);
    // let spec = Body::parse(specfile);
}
