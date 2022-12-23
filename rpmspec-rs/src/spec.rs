use chrono::{DateTime, FixedOffset, NaiveDate};
use pest::{iterators::Pair, Parser};
use pest_derive::Parser;
use std::{collections::BTreeMap, fmt::{Display, Formatter}};

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
struct Changelog {
    entries: Vec<ChangelogEntry>,
}
#[derive(Debug)]
struct ChangelogEntry {
    date: NaiveDate,
    author: String,
    text: Vec<String>,
    version: String,
}

impl Changelog {
    fn new() -> Changelog {
        Changelog {
            entries: Vec::new(),
        }
    }

    fn parse(input: &str) -> Changelog {
        let mut changelog = Changelog::new();
        let mut current_entry = ChangelogEntry::new();
        let mut in_changelog = false;

        for line in input.lines() {
            if line.starts_with("%changelog") {
                in_changelog = true;
            } else if in_changelog {
                if line.starts_with("* ") {
                    if !current_entry.is_empty() {
                        changelog.entries.push(current_entry);
                    }
                    current_entry = ChangelogEntry::new();
                    let parts = line.strip_prefix("* ").unwrap();
                    // println!("parts: {:?}", parts);
                    // example of parts: Dec 06 2022 root - 1.2.0-1
                    // get the date
                    let date_string = parts.split_whitespace().take(4).collect::<Vec<&str>>().join(" ");
                    // println!("date_string: {:?}", date_string);
                    // add filler time to the date string because our string isnt enough to parse
                    current_entry.date = NaiveDate::parse_from_str(&date_string, "%a %b %d %Y").unwrap();
                    // get the author.
                    // we need to split the string by the date string and then take all the elements before -
                    let (author, version) = parts.split_once(" - ").unwrap();
                    let author = author.split_whitespace().skip(4).collect::<Vec<&str>>().join(" ");
                    println!("author: {:?}", author);
                    current_entry.author = author;
                    current_entry.version = version.to_string();
                } else if line.starts_with("- ") {
                    current_entry.text.push(line.strip_prefix("- ").unwrap().to_string());
                }
            }
        }

        if !current_entry.is_empty() {
            changelog.entries.push(current_entry);
        }

        changelog
    }
}

impl Display for Changelog {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut output = String::new();
        output.push_str("%changelog\n");
        for entry in &self.entries {
            output.push_str(&format!(
                "* {} {} - {}\n",
                entry.date.format("%a %b %d %Y"),
                entry.author,
                entry.version
            ));

            for line in &entry.text {
                output.push_str(&format!("- {}\n", line));
            }
            output.push_str("\n");
        }
        write!(f, "{}", output)
    }
}

impl ChangelogEntry {
    fn new() -> ChangelogEntry {
        ChangelogEntry {
            date: NaiveDate::MIN,
            author: String::new(),
            text: Vec::new(),
            version: String::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.date == NaiveDate::MIN
            && self.author.is_empty()
            && self.text.is_empty()
    }
}

#[test]
fn test_sadas() {
    let ch = include_str!("changelog.txt");
    let changelog = Changelog::parse(ch);
    println!("{:#?}", changelog);
    println!("{}", changelog);

    let specfile = include_str!("../../tests/umpkg.spec");
    let spec = SpecParser::parse(Rule::file, specfile).unwrap();
    // println!("{:?}", spec);
    // let spec = Body::parse(specfile);
}
