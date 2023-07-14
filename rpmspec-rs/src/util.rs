use crate::parse::SpecParser;
use smartstring::alias::String;
use std::io::Read;
use tracing::error;

/// string operations / parsing with consumer
///
/// # Requires
/// - `exit!()`
///
/// # Provides
/// - `exit_chk!()`
/// - `back!()`
/// - `next!()`
#[rustfmt::skip] // kamo https://github.com/rust-lang/rustfmt/issues/4609
macro_rules! gen_read_helper {
	($reader:ident $quotes:ident) => {
		macro_rules! exit_chk {
			() => {
				if !$quotes.is_empty() {
					return Err(eyre!("Unclosed quotes: `{}`", $quotes));
				}
			};
		}
		#[allow(unused_macros)]
		macro_rules! next {
			(#) => {{
				let Some(ch) = $reader.next() else { exit!(); };
				ch
			}};
			($c:expr) => {
				if let Some(ch) = $reader.next() {
					textproc::chk_ps(&mut $quotes, ch);
					ch
				} else {
					back!($c);
					exit!();
				}
			};
			(~$c:expr) => {
				if let Some(ch) = $reader.next() {
					ch
				} else {
					$reader.push($c);
					exit!();
				}
			};
		}
	};
	($reader:ident $pa:ident $pb:ident $pc:ident $sq:ident $dq:ident) => {
		($pa, $pb, $pc) = (usize::default(), usize::default(), usize::default());
		($sq, $dq) = (false, false);
		macro_rules! exit_chk {
			() => {
				if $pa != 0 {
					return Err(eyre!("Unclosed `(` while parsing arguments for parameterized macro ({} time(s))", $pa));
				}
				if $pb != 0 {
					return Err(eyre!("Unclosed `[` while parsing arguments for parameterized macro ({} time(s))", $pb));
				}
				if $pc != 0 {
					return Err(eyre!("Unclosed `{{` while parsing arguments for parameterized macro ({} time(s))", $pc));
				}
				if $sq {
					return Err(eyre!("Unclosed `'` while parsing arguments for parameterized macro ({} time(s))", $sq));
				}
				if $dq {
					return Err(eyre!("Unclosed `\"` while parsing arguments for parameterized macro ({} time(s))", $dq));
				}
			};
		}
		#[allow(unused_macros)]
		macro_rules! back {
			($ch:expr) => {
				match $ch {
					'(' => $pa -= 1,
					')' => $pa += 1,
					'[' => $pb -= 1,
					']' => $pb += 1,
					'{' => $pc -= 1,
					'}' => $pc += 1,
					'\'' => $sq = !$sq,
					'"' => $dq = !$dq,
					_ => {}
				}
				$reader.push($ch);
			};
		}
		macro_rules! chk_ps {
			($ch:ident) => {
				match $ch {
					'(' => $pa += 1,
					')' => $pa -= 1,
					'[' => $pb += 1,
					']' => $pb -= 1,
					'{' => $pc += 1,
					'}' => $pc -= 1,
					'\'' => $sq = !$sq,
					'"' => $dq = !$dq,
					_ => {}
				}
			};
		}
		#[allow(unused_macros)]
		macro_rules! quote_remain {
			() => {
				$pa + $pb + $pc != 0 || $sq || $dq
			};
		}
		#[allow(unused_macros)]
		macro_rules! next {
			(#) => {{
				let Some(ch) = $reader.next() else { exit!(); };
				ch
			}};
			($c:expr) => {
				if let Some(ch) = $reader.next() {
					chk_ps!(ch);
					ch
				} else {
					back!($c);
					exit!();
				}
			};
			(~$c:expr) => {
				if let Some(ch) = $reader.next() {
					ch
				} else {
					$reader.push($c);
					exit!();
				}
			}
		}
	};
}

/// A consumer that yields chars from a mutable String.
/// It is a bit more efficient if characters need to be
/// added into the String for the `.next()` iterations.
/// # Implementation
/// `Consumer` internally has `self.s` (String) storing
/// the output of the `BufReader` temporarily. However,
/// it is actually reversed. This is because operations
/// like `pop()` and `push()` are faster (`O(1)`) while
/// `remove(0)` and `insert(0, ?)` are slower (`O(n)`).
#[derive(Debug)]
pub struct Consumer<R: std::io::Read = std::fs::File> {
	s: String,
	r: Option<std::io::BufReader<R>>,
	pub l: usize,
	pub c: usize,
	pub b: usize,
	lastc: usize,
}

impl<R: std::io::Read> Consumer<R> {
	pub fn new(s: &str, r: Option<std::io::BufReader<R>>) -> Self {
		Self { s: s.chars().rev().collect(), r, l: 0, c: 0, b: 0, lastc: 0 }
	}
	pub fn pos(&mut self, l: usize, c: usize, b: usize) {
		self.l = l;
		self.c = c;
		self.b = b;
	}
	#[inline]
	pub fn push(&mut self, c: char) {
		if c == '\n' {
			self.l -= 1;
			self.c = self.lastc;
		} else {
			self.c -= 1;
		}
		self.b -= 1;
		self.s.push(c);
	}
	pub fn read_til_eol(&mut self) -> Option<String> {
		let mut ps = vec![];
		let mut out = String::new();
		macro_rules! close {
			($ch:ident ~ $begin:expr, $end:expr) => {
				if $ch == $end {
					match ps.pop() {
						Some($begin) => continue,
						Some(x) => {
							error!("Found `{}` before closing `{x}`", $end);
							return None;
						}
						None => {
							error!("Unexpected closing char: `{}`", $end);
							return None;
						}
					}
				}
			};
		}
		'main: while let Some(ch) = self.next() {
			if ch == '\0' {
				// idk how it happens
				break;
			}
			if ch == '\n' {
				break;
			}
			if "([{".contains(ch) {
				ps.push(ch);
				continue;
			}
			if ch == '\'' {
				ps.push('\'');
				for ch in self.by_ref() {
					ps.push(ch);
					if ch == '\'' {
						continue 'main;
					}
				}
				error!("Unexpected EOF, `'` not closed");
				return None;
			}
			if ch == '"' {
				ps.push('"');
				for ch in self.by_ref() {
					ps.push(ch);
					if ch == '"' {
						continue 'main;
					}
				}
				error!("Unexpected EOF, `\"` not closed");
				return None;
			}
			close!(ch ~ '(', ')');
			close!(ch ~ '[', ']');
			close!(ch ~ '{', '}');
			out.push(ch);
		}
		if !ps.is_empty() {
			error!("Unclosed: {ps:?}");
			return None;
		}
		if out.is_empty() {
			return None;
		}
		Some(out)
	}
}

impl<R: std::io::Read> Iterator for Consumer<R> {
	type Item = char;

	fn next(&mut self) -> Option<Self::Item> {
		if let Some(c) = self.s.pop() {
			if c == '\n' {
				self.l += 1;
				self.lastc = self.c;
				self.c = 0;
			} else {
				self.c += 1;
			}
			self.b += 1;
			return Some(c);
		}
		if let Some(ref mut r) = self.r {
			let mut buf = [0; 64];
			if r.read(&mut buf).ok()? == 0 {
				None // EOF
			} else {
				self.s = match core::str::from_utf8(&buf) {
					Ok(s) => s.chars().rev().collect(),
					Err(e) => {
						error!("cannot parse buffer `{buf:?}`: {e}");
						return None;
					}
				};
				let c = unsafe { self.s.pop().unwrap_unchecked() };
				if c == '\n' {
					self.l += 1;
					self.lastc = self.c;
					self.c = 0;
				} else {
					self.c += 1;
				}
				self.b += 1;
				Some(c)
			}
		} else {
			None
		}
	}
}

impl<R: std::io::Read> From<&str> for Consumer<R> {
	fn from(value: &str) -> Self {
		Self { s: value.chars().rev().collect(), r: None, l: 0, c: 0, b: 0, lastc: 0 }
	}
}

pub struct SpecMacroParserIter<'a> {
	pub reader: &'a mut Consumer,
	pub parser: &'a mut SpecParser,
	pub percent: bool,
	pub buf: String,
}

impl<'a> Iterator for SpecMacroParserIter<'a> {
	type Item = char;
	fn next(&mut self) -> Option<Self::Item> {
		if !self.buf.is_empty() {
			return self.buf.pop();
		}
		if let Some(ch) = self.reader.next() {
			if ch == '%' {
				self.percent = !self.percent;
				if !self.percent {
					return Some('%');
				}
				return self.next();
			}
			if self.percent {
				self.reader.push(ch);
				match self.parser._use_raw_macro(self.reader) {
					Ok(s) => {
						self.buf = s.chars().rev().collect();
						return self.buf.pop();
					}
					Err(e) => {
						error!("Fail to parse macro: {e:#?}");
						return None;
					}
				}
			}
			return Some(ch);
		}
		None
	}
}

// somehow you need this to export the macro
pub(crate) use gen_read_helper;

pub mod textproc {
	use color_eyre::{eyre::eyre, Result};
	use smartstring::alias::String;
	use tracing::{debug, warn};

	use crate::parse::SpecParser;

	use super::Consumer;

	pub fn chk_ps(quotes: &mut String, ch: char) -> Result<()> {
		if ch == '\'' {
			if quotes.ends_with('\'') {
				quotes.pop();
			} else {
				quotes.push('\'');
			}
		} else if ch == '"' {
			if quotes.ends_with('"') {
				quotes.pop();
			} else {
				quotes.push('"');
			}
		} else if "([{".contains(ch) {
			quotes.push(ch);
		} else if ")]}".contains(ch) {
			match quotes.pop().ok_or_else(|| eyre!("Found `{ch}` but there are no quotes to close"))? {
				'(' if ch != ')' => return Err(eyre!("Expected `)`, Found `{ch}` before closing `(`")),
				'[' if ch != ']' => return Err(eyre!("Expected `]`, Found `{ch}` before closing `['")),
				'{' if ch != '}' => return Err(eyre!("Expected `}}`, Found `{ch}` before closing `{{`")),
				_ => {}
			}
		}
		Ok(())
	}

	pub fn back(reader: &mut Consumer, quotes: &mut String, ch: char) -> Result<()> {
		if ch == '\'' {
			if quotes.ends_with('\'') {
				quotes.pop();
			} else {
				quotes.push('\'');
			}
		}
		if ch == '"' {
			if quotes.ends_with('"') {
				quotes.pop();
			} else {
				quotes.push('"');
			}
		}
		match ch {
			')' => quotes.push('('),
			']' => quotes.push('['),
			'}' => quotes.push('{'),
			'(' if quotes.pop() != Some('(') => return Err(eyre!("BUG: pushing back `(` failed quotes check")),
			'[' if quotes.pop() != Some('[') => return Err(eyre!("BUG: pushing back `[` failed quotes check")),
			'{' if quotes.pop() != Some('{') => return Err(eyre!("BUG: pushing back `{{` failed quotes check")),
			_ => {}
		}
		reader.push(ch);
		Ok(())
	}

	/// Check for `?` and `!` in macro invocation, returns true if processed.
	pub fn flag(question: &mut bool, notflag: &mut bool, ch: char) -> bool {
		if ch == '!' {
			*notflag = !*notflag;
			return true;
		}
		if ch == '?' {
			if *question {
				warn!("Seeing double `?` flag in macro use. Ignoring.");
			}
			*question = true;
			return true;
		}
		false
	}

	/// Expand macros depending on `notflag`.
	///
	/// when %a is undefined, %{!a} expands to %{!a}, but %!a expands to %a.
	pub fn macro_expand_notflagproc(parser: &mut SpecParser, notflag: bool, reader: &mut Consumer, content: &str, name: &str) -> String {
		let out = parser._rp_macro(name, reader);
		if notflag {
			return out.unwrap_or_else(|e| {
				debug!("_rp_macro: {e:#}");
				if content.is_empty() { format!("%{name}") } else { format!("%{{!{name}}}") }.into()
			});
		}
		out.unwrap_or_default()
	}
}
