//! [`ParserError`] used in rpmspec-rs.
//! Yes. You heard me. The only error is [`ParserError`] and everything else is
//! unfortunately String.
use smartstring::alias::String;

/// Errors for some special parsing issues
#[derive(Debug)]
#[allow(clippy::module_name_repetitions)]
pub enum ParserError {
	/// A preamble is required but it is not found.
	NoPreamble(String),
	/// The preamble specified is invalid.
	UnknownPreamble(usize, String),
	/// A preamble that cannot be specified more than once was found duplicate.d
	Duplicate(usize, String),
	/// A modifier (such as `pre` in `Requires(pre):`) is found to be invalid.
	UnknownModifier(usize, String),
	/// A macro was not found. If you see this, probably something wrong with rpmspec-rs.
	UnknownMacro(usize, String),
	/// A color_eyre::Report. Some sort of syntax error.
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
			Self::NoPreamble(name) => write!(f, "! Preamble not found for {name}"),
			Self::UnknownPreamble(line, name) => {
				write!(f, "! {line}: Unknown Preamble for {name}")
			}
			Self::Duplicate(line, name) => write!(f, "! {line}: Duplicate Preamble for {name}"),
			Self::UnknownModifier(line, name) => {
				write!(f, "! {line}: Unknown Modifier for {name}")
			}
			Self::UnknownMacro(line, name) => write!(f, "! {line}: Unknown Macro for {name}"),
			Self::Others(r) => write!(f, "! {r:#}"),
		}
	}
}
