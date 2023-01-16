use smartstring::alias::CompactString;
use tracing::{debug, error, instrument};

use super::{macros::{find_macro_end, SaiGaai}, rpmhook::RPMHookArgs};

const RPMEXPR_EXPAND: i8 = 1 << 0;
const RPMEXPR_DISCARD: i8 = 1 << 31; // internal, discard result

#[derive(Debug)]
struct ParseState {
	s: String,          // expr string
	p: String,          // cur. pos. in expr str
	next_token: Token,  // cur. lookahead token
	token_value: Value, // valid when TOK_INTEGER or TOK_STRING
	flags: i8,
}
#[derive(Clone, Debug, Default)]
pub(crate) enum Value {
	String(String),
	Int(i64), // nyeshu
	Rpmver(RPMVer),
	Bool(bool), // they didn't have this for some absurd unknown reasons
	#[default]
	Nil,
}
#[derive(Clone, Debug, Default)]
struct RPMVer {
	e: CompactString,
	v: CompactString,
	r: CompactString,
	arena: String,
}
impl From<String> for RPMVer {
	fn from(value: String) -> Self {
		let mut rv = Self::default();
		rv.arena = value;
		let (a, _) = rv
			.arena
			.split_once(|a: char| !a.is_ascii_digit())
			.unwrap_or(("", ""));
		let s = rv.arena.chars().nth(a.len()).unwrap_or('\0'); // epoch terminator
		let last = rv.arena.split('-').last().unwrap_or("");
		let se = &rv.arena[rv.arena.len() - last.len() - 1..]; // version terminator
		if s == ':' {
			rv.e = rv.arena.into();
			rv.v = CompactString::from(a);
			if rv.e.is_empty() {
				rv.e = CompactString::from("0");
			} else {
				rv.e = CompactString::new();
				rv.v = rv.arena.into();
			}
			if !se.is_empty() {
				rv.r = se[1..].into();
			} else {
				rv.r = CompactString::new();
			}
		}
		rv
	}
}
#[derive(Debug, PartialEq)]
enum Token {
	Unknown,  // 0
	EOF,
	Add,
	Minus,
	Mul,
	Div,
	OpenP,
	CloseP,
	Eq,
	NEq,
	Not,
	LE,
	LT,
	GE,
	GT,
	LogicalAnd,
	LogicalOr,
	TenaryCond,
	TenaryAlt,
	Comma,
	Function,
	Integer,
	String,
	Version,
}

impl Token {
	fn symbol(&self) -> &str {
		use Token::*;
		match self {
			EOF => "EOF",
			Integer => "I",
			String => "S",
			Add => "+",
			Minus => "-",
			Mul => "*",
			Div => "/",
			OpenP => "( ",
			CloseP => " )",
			Eq => "==",
			NEq => "!=",
			LT => "<",
			LE => "<=",
			GE => ">=",
			NOT => "!",
			LOGICAL_AND => "&&",
			LOGICAL_OR => "||",
			TERNARY_COND => "?",
			TERNARY_ALT => ":",
			VERSION => "V",
			COMMA => ",",
			FUNCTION => "f( ",
		}
	}
}

/// true: error!!!
fn rd_token(state: &mut ParseState) -> bool {
	let token;
	let mut v = Value::Nil;
	let ps = state.p.clone();
	let expand = (state.flags & RPMEXPR_EXPAND) != 0;

	// -> skip whitespace before next token
	let mut p = ps.trim_start(); // :3

	if p.len() == 0 {
		token = Token::EOF;
		p = &ps[p.len() - 1..];
	} else {
		token = match p.chars().nth(0).unwrap_or('\0') {
			'+' => Token::Add,
			'-' => Token::Minus,
			'*' => Token::Mul,
			'/' => Token::Div,
			'(' => Token::OpenP,
			')' => Token::CloseP,
			'=' => {
				if p.chars().nth(1) == Some('=') {
					p = &p[1..];
					Token::Eq
				} else {
					expr_err(state, "syntax error while parsing ==", &p[2..]);
					return true;
				}
			}
			'!' => {
				if p.chars().nth(1) == Some('=') {
					p = &p[1..];
					Token::NEq
				} else {
					Token::Not
				}
			}
			'<' => {
				if p.chars().nth(1) == Some('=') {
					p = &p[1..];
					Token::LE
				} else {
					Token::LT
				}
			}
			'>' => {
				if p.chars().nth(1) == Some('=') {
					p = &p[1..];
					Token::GE
				} else {
					Token::GT
				}
			}
			'&' => {
				if p.chars().nth(1) == Some('&') {
					p = &p[1..];
					Token::LogicalAnd
				} else {
					expr_err(state, "syntax error while parsing &&", &p[2..]);
					return true;
				}
			}
			'|' => {
				if p.chars().nth(1) == Some('|') {
					p = &p[1..];
					Token::LogicalOr
				} else {
					expr_err(state, "syntax error while parsing ||", &p[2..]);
					return true;
				}
			}
			'?' => Token::TenaryCond,
			':' => Token::TenaryAlt,
			',' => Token::Comma,
			a => {
				if a.is_ascii_digit() || (a == '%' && expand) {
					let mut ts: usize = 0;
					while let Some(ch) = p.chars().nth(ts) {
						if ch == '%' && expand {
							ts = skipMacro(p, ts + 1) - 1;
						} else if !ch.is_ascii_digit() {
							break;
						}
					}
					let tmp = get_valuebuf(state, p, ts);
					if tmp.is_empty() {
						return false;
					}
					// -> make sure expanded buf only contains digits
					if expand && !wellformed_integer(&tmp) {
						if let Some(c) = tmp.chars().nth(0) {
							if c.is_ascii_alphabetic() {
								expr_err(
									state,
									"macro expansion returned a bare word, please use \"...\"",
									&p[1..],
								);
							}
						} else {
							expr_err(state, "macro expansion did not return an integer", &p[1..]);
						}
						error!("expanded string: {tmp}");
					}
					p = &p[ts - 1..];
					v = Value::Int(tmp.parse().expect("can't conv str (known int) to int"));
					Token::Integer
				} else if p.starts_with('\"') || (p.starts_with("v\"")) {
					let tmp;
					let ts: usize;
					let qtok;
					if p.starts_with('v') {
						qtok = Token::Version;
						p = &p[2..];
					} else {
						qtok = Token::String;
						p = &p[1..];
					}
					let mut ts: usize = 0;
					while let Some(ch) = p.chars().nth(ts) {
						if ch == '%' && expand {
							ts = skipMacro(p, ts + 1) - 1;
						} else if ch == '\"' {
							break;
						}
					}
					if p.chars().nth(ts) != Some('\"') {
						expr_err(state, "unterminated string in expression", &p[ts + 1..]);
						// goto err
					}
					tmp = get_valuebuf(state, p, ts);
					if tmp.is_empty() {
						return true;
					}
					p = &p[ts..];
					if qtok == Token::String {
						v = Value::String(tmp);
					} else {
						let rpmver = RPMVer::from(if (state.flags & RPMEXPR_DISCARD) == 0 {
							tmp
						} else {
							"0".into()
						});
						if rpmver.v.is_empty() {
							expr_err(state, "invalid version", &p[1..]);
							return true;
						}
						v = Value::Rpmver(rpmver);
					}
					Token::String
				} else if p.chars().nth(0).unwrap_or('\0').is_ascii_alphabetic() {
					let pe = isFunctionCall(p);
					if !pe.is_empty() {
						// todo is this check useless?
						if pe.starts_with('(') {
							v = Value::String(p[..p.len() - pe.len()].to_string());
							p = pe;
							Token::Function
						} else {
							expr_err(
								state,
								"bare words are no longer supported, please use \"...\"",
								&p[1..],
							);
							return true;
						}
					} else {
						expr_err(
							state,
							"bare words are no longer supported, please use \"...\"",
							&p[1..],
						);
						return true;
					}
				} else {
					expr_err(state, "parse error in expression", &p[1..]);
					return true;
				}
			}
		}
	}
	state.p = p[1..].to_string();
	state.next_token = token;
	state.token_value = v;
	debug!("rdToken: `{}` ({token:?})", token.symbol());
	debug!("rdToken: {:?}", state.token_value);
	false
}

fn skipMacro(p: &str, ts: usize) -> usize {
	if p.starts_with('%') {
		ts + 1
	} else {
		let pe = find_macro_end(&p[ts..]);
		let pe = &p[pe..];
		if pe.is_empty() {
			p.len()
		} else {
			p.len() - pe.len()
		}
	}
}
fn isFunctionCall(p: &str) -> &str {
	if !p.starts_with(|p: char| p.is_ascii_alphabetic()) && p.chars().nth(1) != Some('_') {
		""
	} else {
		let p = p.trim_start_matches(|p: char| p.is_ascii_alphanumeric() || "_.:".contains(p));
		if p.starts_with('(') {
			p
		} else {
			""
		}
	}
}

#[deprecated(note = "manually create Value::Int")]
fn value_make_integer() {}

fn wellformed_integer(mut p: &str) -> bool {
	if p.starts_with('-') {
		p = &p[1..];
	}
	for c in p.chars() {
		if !c.is_ascii_digit() {
			return false;
		}
	}
	true
}

fn get_valuebuf(state: &ParseState, p: &str, mut size: usize) -> String {
	let mut tmp = String::with_capacity(size);
	if (state.flags & RPMEXPR_DISCARD) != 0 {
		size = 0
	}
	tmp += &p[..size];
	if size > 0 && (state.flags & RPMEXPR_EXPAND) != 0 {
		let tmp2 = String::new();
		// SaiGaai::expandMacros(None, src);
		todo!();
		tmp2
	} else {
		tmp
	}
}
#[instrument]
fn do_primary(state: &mut ParseState) -> Value {
	let p = state.p.clone();
	debug!("start");
	use Token::*;
	let v = match state.next_token {
		Function => do_function(state),
		OpenP => {
			if rd_token(state) {
				Value::Nil
			} else {
				let v = do_tenary(state);
				if state.next_token != CloseP {
					expr_err(state, "unmatched (", &p);
					Value::Nil
				} else if rd_token(state) {
					Value::Nil
				} else {
					v
				}
			}
		}
		Integer | String => {
			let v = state.token_value.clone();
			if rd_token(state) {
				Value::Nil
			} else {
				v
			}
		}
		Minus => {
			if rd_token(state) {
				Value::Nil
			} else {
				let v = do_primary(state);
				if let Value::Int(i) = v {
					Value::Int(-i)
				} else {
					expr_err(state, "- only on numbers", &p);
					Value::Nil
				}
			}
		}
		Not => {
			if rd_token(state) {
				Value::Nil
			} else {
				let v = do_primary(state);
				if let Value::Nil = v {
					Value::Nil
				} else {
					Value::Bool(boolify_value(v))
				}
			}
		}
		EOF => {
			expr_err(state, "unexpected end of expression", &state.p);
			Value::Nil
		}
		_ => {
			expr_err(state, "syntax error in expression", &state.p);
			Value::Nil
		}
	};
	debug!("{v:?}");
	v
}

fn boolify_value(v: Value) -> bool {
	if let Value::Int(i) = v {
		i != 0
	} else if let Value::String(s) = v {
		!s.is_empty()
	} else {
		false
	}
}

#[instrument]
fn do_function(state: &mut ParseState) -> Value {
	let vname = state.token_value.clone();
	let mut v = Value::Nil;
	if rd_token(state) {
		return Value::Nil;
	}
	let mut varg: Vec<Value> = vec![];
	let mut narg = 0;
	while state.next_token != Token::CloseP {
		let a = do_tenary(state);
		if let Value::Nil = a {
			return Value::Nil;
		} else {
			varg[narg] = a;
			narg += 1;
			if state.next_token == Token::CloseP {
				break;
			}
			if state.next_token != Token::Comma {
				expr_err(state, "syntax error in expression", &state.p);
				return Value::Nil;
			}
			if rd_token(state) {
				return Value::Nil;
			}
			if state.next_token == Token::CloseP {
				expr_err(state, "syntax error in expression", &state.p);
				return Value::Nil;
			}
		}
	}
	if rd_token(state) {
		return Value::Nil;
	}
	// -> Do the call
	if let Value::String(s) = vname {
		if &s[0..4] == "lua:" {
			return do_lua_function(state, &s[4..], narg, varg);
		} else {
			expr_err(state, "unsupported function", &state.p);
		}
	}
	v
}

#[instrument]
fn do_lua_function(state: &ParseState, name: &str, argc: usize, argv: Vec<Value>) -> Value {
	let lua;
	let args;
	let v;
	let result: &str;
	let argt: &str;
	let i;

	if (state.flags & RPMEXPR_DISCARD) != 0 {
		return Value::String("".into());
	}
	let args = argv;
	
}

#[instrument]
fn do_multiply_divide(state: &ParseState) -> Value {
	debug!("start");
}

#[instrument]
fn do_add_subtract(state: &ParseState) -> Value {
	debug!("start");
}

#[instrument]
fn do_relational(state: &ParseState) -> Value {
	debug!("start");
}

fn do_logical(state: &ParseState) -> Value {
	let oldflags = state.flags;
	debug!("do_logical()");
}

fn do_tenary(state: &ParseState) -> Value {
	let oldflags = state.flags;
}

fn expr_err(state: &ParseState, msg: &str, mut p: &str) {
	let newLine = state.s.find('\n');
	if let Some(newLine) = newLine {
		if state.s.len() == newLine + 1 {
			p = "";
		}
	}
	error!("{msg}: {}", state.s);
	if !p.is_empty() {
		let l = state.s.len() - p.len() + msg.len() + 2;
		error!("{}^", " ".repeat(l));
	}
}

fn rpm_expr_str_flags(expr: &str, flags: i8) -> Option<String> {
	// -> Init. expr parser state
	let mut state = ParseState {
		p: expr.into(),
		s: expr.into(),
		next_token: Token::Unknown,
		token_value: Value::Nil,
		flags,
	};

	if rd_token(&mut state) {
		return None;
	}

	todo!()
}
