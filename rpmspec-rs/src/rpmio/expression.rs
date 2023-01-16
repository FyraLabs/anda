use smartstring::alias::CompactString;
use tracing::{debug, instrument,error};

use super::macros::SaiGaai;

const RPMEXPR_EXPAND: i8 = 1 << 0;
const RPMEXPR_DISCARD: i8 = 1 << 31; // internal, discard result

#[derive(Debug)]
struct ParseState {
	s: String,          // expr string
	p: String,          // cur. pos. in expr str
	next_token: Token,     // cur. lookahead token
	token_value: Value, // valid when TOK_INTEGER or TOK_STRING
	flags: i8,
}
#[derive(Debug)]
enum Value {
	String(String),
	Int(u64), // nyeshu
	Rpmver(RPMVer),
	None,
}
#[derive(Debug, Default)]
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
		let epoch: CompactString;
		let version: CompactString;
		let release: CompactString;
		let (a, _) = rv.arena.split_once(|a: char| !a.is_ascii_digit()).unwrap_or(("", ""));
		let s = rv.arena.chars().nth(a.len()).unwrap_or('\0'); // epoch terminator
		let last= rv.arena.split('-').last().unwrap_or("");
		let se = &rv.arena[rv.arena.len() - last.len() - 1..]; // version terminator
		if s == ':' {
			epoch = rv.arena.into();
			version = CompactString::from(a);
			if epoch.is_empty() {
				epoch = CompactString::from("0");
			} else {
				epoch = CompactString::new();
				version = rv.arena.into();
			}
			if !se.is_empty() {
				release = se[1..].into();
			} else {
				release = CompactString::new();
			}
		}
		rv.e = epoch;
		rv.v = version;
		rv.r = release;
		rv
	}
}
#[derive(Debug, PartialEq)]
enum Token {
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

fn rdToken(state: &ParseState) -> bool {
	let token;
	let v = Value::None;
	let ps = state.p;
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
					todo!()
					// exprErr(state, "syntax error while parsing ==", p+2);
					RPMEXPR_DISCARD		// goto err;
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
					todo!()
					// exprErr(state, "syntax error while parsing &&", p+2);
					// goto err;
				}
			}
			'|' => {
				if p.chars().nth(1) == Some('|') {
					p = &p[1..];
					Token::LogicalOr
				} else {
					todo!()
					// exprErr(state, "syntax error while parsing ||", p+2);
					// goto err;
				}
			}
			'?' => Token::TenaryCond,
			':' => Token::TenaryAlt,
			',' => Token::Comma,
			a => {
				if a.is_ascii_digit() || (a == '%' && expand) {
					let mut ts: usize;
					while let Some(ch) = p.chars().nth(ts) {
						if ch == '%' && expand {
							ts = skipMacro(p, ts + 1) - 1;
						} else if !ch.is_ascii_digit() {
							break;
						}
					}
					let tmp = getValuebuf(state, p, ts);
					if tmp.is_empty() {return false;}
					// -> make sure expanded buf only contains digits
					if expand && !wellformedInteger(&tmp) {
						if let Some(c) = tmp.chars().nth(0) {
							if c.is_ascii_alphabetic() {
								exprErr(state, "macro expansion returned a bare word, please use \"...\"", &p[1..]);
							}
						} else {
							exprErr(state, "macro expansion did not return an integer", &p[1..]);
						}
						error!("expanded string: {tmp}");
					}
					p = &p[ts-1..];
					v = Value::Int(tmp.parse().expect("can't conv str (known int) to int"));
					Token::Integer
				} else if p.starts_with('\"') || (p.starts_with("v\"")) {
					let tmp;
					let ts;
					let qtok;
					if p.starts_with('v') {
						qtok = Token::Version;
						p = &p[2..];
					} else {
						qtok = Token::String;
						p = &p[1..];
					}


					let mut ts: usize;
					while let Some(ch) = p.chars().nth(ts) {
						if ch == '%' && expand {
							ts = skipMacro(p, ts + 1) - 1;
						} else if ch == '\"' {
							break;
						}
					}
					if p.chars().nth(ts) != Some('\"') {
						exprErr(state, "unterminated string in expression", &p[ts+1..]);
						// goto err
					}
					tmp = getValuebuf(state, p, ts);
					if tmp.is_empty() {
						return true;
					}
					p = &p[ts..];
					if qtok == Token::String {
						v = Value::String(tmp);
					} else {
						let rpmver = RPMVer::from(if (state.flags & RPMEXPR_DISCARD) == 0 {tmp} else {"0".into()});
						if rpmver.v.is_empty() {
							exprErr(state, "invalid version", &p[1..]);
							return true;
						}
						v = Value::Rpmver(rpmver);
					}
					Token::String
				} else if p.chars().nth(0).unwrap_or('\0').is_ascii_alphabetic() {
					let pe = isFunctionCall(p);
					if let Some(pe) = pe {
						if pe.startswith('(') {
							v = Value::String(p[..p.len()-pe.len()].to_string());
							p = pe;
							Token::Function
						} else {
							exprErr(state, "bare words are no longer supported, please use \"...\"", &p[1..]);
					return true;
						}
					} else {
					exprErr(state, "bare words are no longer supported, please use \"...\"", &p[1..]);
					return true;}
				} else {
					exprErr(state, "parse error in expression", &p[1..]);
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

#[deprecated(note = "manually create Value::Int")]
fn valueMakeInteger() {}

fn wellformedInteger(mut p: &str) -> bool {
	if p.starts_with('-') {
		p = &p[1..];
	}
	for c in p.chars() { 
		if !c.is_ascii_digit() {
			return false
		}
	}
	true
}

fn getValuebuf(state: &ParseState, p: &str, mut size: usize) -> String {
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
	tmp}
}
#[instrument]
fn doPrimary(state: &ParseState) -> Value {
	let p = state.p;
	debug!("start");
	use Token::*;
	todo!();
	match state.next_token {
		Function => {
			let v = doFunction(state);
		}
	}
}

#[instrument]
fn doFunction(state: &ParseState) -> Value {
	let vname = state.token_value;
	let mut v = Value::None;
	if rdToken(state) {
		return Value::None
	}
	let mut varg: Vec<Value> = vec![];
	let mut narg=  0;
	while state.next_token != Token::CloseP {
		if let Some(a) = doTenary(state) {
			varg[narg] = a;
			narg += 1;
			if state.next_token == Token::CloseP {
				break;
			}
			if state.next_token != Token::Comma {
				exprErr(state, "syntax error in expression", &state.p);
				return Value::None
			}
			if rdToken(state) {
				return Value::None
			}
			if state.next_token == Token::CloseP {
				exprErr(state, "syntax error in expression", &state.p);
				return Value::None
			}
		} else {
			return Value::None
		}
	}
	if rdToken(state) { 
		return Value::None
	}
	// -> Do the call
	if let Value::String(s) = vname {
		if &s[0..4] == "lua:" {
			return doLuaFunction(state, &s[4..], narg, varg);
		} else {
			exprErr(state, "unsupported function", &state.p);
		}
	}
	v
}

#[instrument]
fn doLuaFunction(state: &ParseState, name: &str, argc: usize, argv: Vec<Value>) -> Value {
	let lua;
	let args;
	let v;
	let result: &str;
	let argt: &str;
	let i;

	if (state.flags & RPMEXPR_DISCARD) != 0 {
		return valueMakeString("");
	}
}

#[instrument]
fn doMultiplyDivide(state: &ParseState) -> Value {
	debug!("start");
	
}

#[instrument]
fn doAddSubtract(state: &ParseState) -> Value {
	debug!("start");

}

#[instrument]
fn doRelational(state: &ParseState) -> Value {
	debug!("start");
}


fn doLogical(state: &ParseState) -> Value {
	let oldflags = state.flags;
	debug!("doLogical()");
}

fn doTenary(state: &ParseState) -> Option<Value> {
	let oldflags = state.flags;
}

fn exprErr(state: &ParseState, msg: &str, mut p: &str) {
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

fn rpmExprStrFlags(expr: &str, flags: i8) -> Option<String> {
	// -> Init. expr parser state
	let state = ParseState {
		p: expr.into(),
		s: expr.into(),
		next_token: 0,
		token_value: Value::None,
		flags,
	};
	if rdToken(&state) {return None;}
}
