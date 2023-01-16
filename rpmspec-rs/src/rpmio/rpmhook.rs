use super::expression::Value;

#[derive(Debug, Default)]
#[deprecated(note = "literally a Vec<Value>")]
pub(crate) struct RPMHookArgs {
	pub(crate) argt: String,
	pub(crate) argv: Vec<Value>,
}

impl RPMHookArgs {
	pub(crate) fn new() -> Self {
		Self {
			..Default::default()
		}
	}
}
