use std::io::Stderr;

#[derive(Debug)]
pub enum ParserError {
    NoPreamble(String),
    UnknownPreamble(usize, String),
    Duplicate(usize, String),
    UnknownModifier(usize, String),
    UnknownMacro(usize, String),
}

impl std::error::Error for ParserError {
    fn description(&self) -> &str {
        match self {
            Self::NoPreamble(_) => "Preamble required but not found",
            Self::UnknownPreamble(_, _) => "Preamble not supported",
            Self::Duplicate(_, _) => "Preamble unexpectedly duplicated",
            Self::UnknownModifier(_, _) => "Modifier not supported",
            Self::UnknownMacro(_, _) => "Macro not supported",
        }
    }
}

impl std::fmt::Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::NoPreamble(name) => write!(f, "! Preamble not found for {}", name),
            Self::UnknownPreamble(line, name) => write!(f, "! {}: Unknown Preamble for {}", line, name),
            Self::Duplicate(line, name) => write!(f, "! {}: Duplicate Preamble for {}", line, name),
            Self::UnknownModifier(line, name) => write!(f, "! {}: Unknown Modifier for {}", line, name),
            Self::UnknownMacro(line, name) => write!(f, "! {}: Unknown Macro for {}", line, name),
        }
    }
}
