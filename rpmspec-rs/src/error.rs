use smartstring::alias::String;

#[derive(Debug)]
pub enum ParserError {
	NoPreamble(String),
	UnknownPreamble(usize, String),
	Duplicate(usize, String),
	UnknownModifier(usize, String),
	UnknownMacro(usize, String),
	Others(color_eyre::Report),
}

impl Clone for ParserError {
	fn clone(&self) -> Self {
		match self {
			Self::NoPreamble(s) => Self::NoPreamble(s.clone()),
			Self::UnknownPreamble(a, b) => Self::UnknownPreamble(*a, b.clone()),
			Self::Duplicate(a, b) => Self::Duplicate(*a, b.clone()),
			Self::UnknownModifier(a, b) => Self::UnknownModifier(*a, b.clone()),
			Self::UnknownMacro(a, b) => Self::UnknownMacro(*a, b.clone()),
			Self::Others(r) => {
				tracing::warn!("Cloning ParserError::Others(color_eyre::Report):\n{r:#}");
				Self::Others(color_eyre::eyre::eyre!(r.to_string()))
			}
		}
	}
}

impl std::error::Error for ParserError {
	fn description(&self) -> &str {
		match self {
			Self::NoPreamble(_) => "Preamble required but not found",
			Self::UnknownPreamble(_, _) => "Preamble not supported",
			Self::Duplicate(_, _) => "Preamble unexpectedly duplicated",
			Self::UnknownModifier(_, _) => "Modifier not supported",
			Self::UnknownMacro(_, _) => "Macro not found",
			Self::Others(_) => "Parsing Error",
		}
	}
}

impl std::fmt::Display for ParserError {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
			Self::NoPreamble(name) => write!(f, "! Preamble not found for {}", name),
			Self::UnknownPreamble(line, name) => {
				write!(f, "! {}: Unknown Preamble for {}", line, name)
			}
			Self::Duplicate(line, name) => write!(f, "! {}: Duplicate Preamble for {}", line, name),
			Self::UnknownModifier(line, name) => {
				write!(f, "! {}: Unknown Modifier for {}", line, name)
			}
			Self::UnknownMacro(line, name) => write!(f, "! {}: Unknown Macro for {}", line, name),
			Self::Others(r) => write!(f, "! {r:#}"),
		}
	}
}
