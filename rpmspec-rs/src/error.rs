#[derive(Debug)]
pub enum ParserError {
    NoPreamble(String),
    UnknownPreamble(usize, String),
    Duplicate(usize, String),
}

impl std::error::Error for ParserError {
    fn description(&self) -> &str {
        match self {
            Self::NoPreamble(_) => "Preamble required but not found",
            Self::UnknownPreamble(_, _) => "Preamble not supported",
            Self::Duplicate(_, _) => "Preamble unexpectedly duplicated",
        }
    }
}

impl std::fmt::Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::NoPreamble(name) => write!(f, "! Preamble not found for {}", name),
            Self::UnknownPreamble(line, name) => write!(f, "! {}: Unknown Preamble for {}", line, name),
            Self::Duplicate(line, name) => write!(f, "! {}: Duplicate Preamble for {}", line, name),
        }
    }
}
