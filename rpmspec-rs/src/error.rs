pub enum ParserError {
    NoPreamble(&'static str),
    UnknownPreamble(&'static str, &'static str)
}

impl std::fmt::Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::NoPreamble(name) => write!(f, "Preamble not found for {}", name),
            Self::UnknownPreamble(name, value) => write!(f, "Unknown Preamble for {}: {}", name, value),
        }
    }
}
