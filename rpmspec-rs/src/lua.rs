#![warn(clippy::disallowed_types)]
/// Handlers for Lua \
/// FYI RPM spec has tight integration with Lua \
///
/// BTW everything here is heavily rewritten to use `rlua`
///
/// original C code creates a lib which is not really supported
/// in rlua so we kinda have to register a custom type instead
/// aka. `rlua::UserData` \
/// there are two things: `rpm` and `posix`. See more information:
/// https://rpm-software-management.github.io/rpm/manual/lua.html
use parking_lot::Mutex;
use rlua::Lua;
use std::{fmt::Write, sync::Arc};

use base64::{engine::general_purpose::STANDARD, Engine};
use rlua::{Context, ExternalError, Result};

use crate::parse::SpecParser;

use repl::repl;

mod repl;

pub struct RPMLua<'a> {
	pub rpmparser: &'a SpecParser,
	pub luaparser: Lua,
}

impl RPMLua<'_> {
	pub(crate) fn b64decode(_: Context, arg: String) -> Result<String> {
		String::from_utf8(STANDARD.decode(arg).map_err(|e| e.to_lua_err())?).map_err(|e| e.to_lua_err())
	}
	pub(crate) fn b64encode(_: Context, arg: String) -> Result<String> {
		Ok(STANDARD.encode(arg))
	}
	pub(crate) fn call(_: Context, _arg: String) -> Result<()> {
		todo!()
	}
	pub(crate) fn define(rpmparser: Arc<Mutex<SpecParser>>, _: Context, arg: String) -> Result<()> {
		if let Some((name, def)) = arg.split_once(' ') {
			let mut def: String = def.into();
			let name: String = if let Some(name) = name.strip_suffix("()") {
				def.push(' ');
				name.into()
			} else {
				name.into()
			};
			let mut p = rpmparser.lock();
			p.macros.insert(name.into(), def.into());
			Ok(())
		} else {
			Err("Invalid syntax: `%define {def}`".to_lua_err())
		}
	}
	pub(crate) fn execute(_: Context, args: Vec<String>) -> Result<i32> {
		Ok(std::process::Command::new(&args[0]).args(&args[1..]).status().map_err(|e| e.to_lua_err())?.code().unwrap_or(-1))
	}
	pub(crate) fn expand(rpmparser: Arc<Mutex<SpecParser>>, _: Context, arg: &str) -> Result<String> {
		let mut p = rpmparser.lock();
		Ok(p.parse_macro(&mut arg.into()).collect::<String>())
	}
	pub(crate) fn interactive(_: Context, _: String) -> Result<()> {
		repl(); // lazy
		// todo mimic
		Ok(())
	}
	pub(crate) fn isdefined(rpmparser: Arc<Mutex<SpecParser>>, _: Context, name: &str) -> Result<(bool, bool)> {
		if let Some(def) = rpmparser.lock().macros.get(name) {
			return Ok((true, def.ends_with(' ')));
		}
		Ok((false, false))
	}
	pub(crate) fn load(rpmparser: Arc<Mutex<SpecParser>>, _: Context, arg: String) -> Result<()> {
		rpmparser.lock().load_macro_from_file(std::path::PathBuf::from(arg)).map_err(|e| e.to_lua_err())
	}
	pub(crate) fn redirect2null(_: Context, _arg: i32) -> Result<()> {
		todo!()
	}
	pub(crate) fn register(_: Context, _arg: String) -> Result<()> {
		todo!()
	}
	pub(crate) fn undefine(rpmparser: Arc<Mutex<SpecParser>>, _: Context, name: String) -> Result<()> {
		rpmparser.lock().macros.remove(&*name).ok_or_else(|| "error undefining macro".to_lua_err())?;
		Ok(())
	}
	pub(crate) fn unregister(_: Context, _arg: String) -> Result<()> {
		todo!()
	}
	pub(crate) fn vercmp(_: Context, (_s1, _s2): (String, String)) -> Result<()> {
		todo!()
	}
	pub(crate) fn run(rpmparser: Arc<Mutex<SpecParser>>, script: &str) -> Result<String> {
		let lua = Lua::new();
		let anda_out = Arc::new(Mutex::new(String::new()));
		lua.context(|ctx| -> rlua::Result<()> {
			let rpm = ctx.create_table()?;
			rpm.set("b64encode", ctx.create_function(Self::b64encode)?)?;
			rpm.set("b64decode", ctx.create_function(Self::b64decode)?)?;
			let p = rpmparser.clone();
			rpm.set("expand", ctx.create_function(move |ctx, arg: String| Self::expand(p.clone(), ctx, &arg))?)?;
			let p = rpmparser.clone();
			rpm.set("define", ctx.create_function(move |ctx, arg| Self::define(p.clone(), ctx, arg))?)?;
			let p = rpmparser.clone();
			rpm.set("undefine", ctx.create_function(move |ctx, arg| Self::undefine(p.clone(), ctx, arg))?)?;
			let p = rpmparser.clone();
			rpm.set("isdefined", ctx.create_function(move |ctx, arg: String| Self::isdefined(p.clone(), ctx, &arg))?)?;
			let p = rpmparser.clone();
			rpm.set("load", ctx.create_function(move |ctx, arg| Self::load(p.clone(), ctx, arg))?)?;
			rpm.set("register", ctx.create_function(Self::register)?)?;
			rpm.set("unregister", ctx.create_function(Self::unregister)?)?;
			rpm.set("call", ctx.create_function(Self::call)?)?;
			rpm.set("interactive", ctx.create_function(Self::interactive)?)?;
			rpm.set("execute", ctx.create_function(Self::execute)?)?;
			rpm.set("redirect2null", ctx.create_function(Self::redirect2null)?)?;
			rpm.set("vercmp", ctx.create_function(Self::vercmp)?)?;
			// rpm.set("ver", ctx.create_function(Self::ver_new)?)?;
			// rpm.set("open", ctx.create_function(Self::open)?)?;
			// rpm.set("splitargs", ctx.create_function(Self::splitargs)?)?;
			// rpm.set("unsplitargs", ctx.create_function(Self::unsplitargs)?)?;

			let globals = ctx.globals();
			globals.set("rpm", rpm)?;
			let anda_out = anda_out.clone();
			globals.set(
				"print",
				ctx.create_function(move |_, s: String| {
					anda_out.lock().write_str(&s).map_err(|e| e.to_lua_err())?;
					Ok(())
				})?,
			)?;
			ctx.load(script).exec()?;
			Ok(())
		})?;
		Ok(Arc::try_unwrap(anda_out).unwrap().into_inner())
	}
}
