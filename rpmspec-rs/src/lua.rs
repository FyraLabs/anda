mod repl;

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
use rlua::Lua;

mod lua_rpm {
	use std::process::Command;

	use base64::{engine::general_purpose::STANDARD, Engine};
	use rlua::{Context, ExternalError, Result};

	use super::repl::repl;

	pub(crate) fn b64decode(_: Context, arg: String) -> Result<String> {
		String::from_utf8(STANDARD.decode(arg).map_err(|e| e.to_lua_err())?)
			.map_err(|e| e.to_lua_err())
	}
	pub(crate) fn b64encode(_: Context, arg: String) -> Result<String> {
		Ok(STANDARD.encode(arg))
	}
	pub(crate) fn call(_: Context, arg: String) {
		todo!()
	}
	pub(crate) fn define(_: Context, name: String) -> Result<()> {
		define_macro(None, &name, 0).map_err(|_| "error defining macro".to_lua_err())?;
		Ok(())
	}
	pub(crate) fn execute(_: Context, args: Vec<String>) -> Result<i32> {
		Ok(Command::new(&args[0])
			.args(&args[1..])
			.status()
			.map_err(|e| e.to_lua_err())?
			.code()
			.unwrap_or(-1))
	}
	pub(crate) fn expand(_: Context, arg: String) -> Result<String> {
		expand_macros(_dummy_context(), &arg, 0).map_err(|e| e.to_lua_err())
	}
	pub(crate) fn interactive(ctx: Context) -> Result<()> {
		repl(); // lazy
		// todo mimic
		Ok(())
	}
	pub(crate) fn isdefined(_: Context, name: String) -> Result<(bool, bool)> {
		let a = if let Ok(true) = macro_is_defined(None, &name) { true } else { false };
		let b = if let Ok(true) = macro_is_parametric(None, &name) { true } else { false };
		Ok((a, b))
	}
	pub(crate) fn load(_: Context, arg: String) -> Result<i32> {
		load_macro_file(None, &arg).map_err(|e| e.to_lua_err())
	}
	pub(crate) fn redirect2null(_: Context, arg: i32) {
		todo!()
	}
	pub(crate) fn register(_: Context, arg: String) {
		todo!()
	}
	pub(crate) fn undefine(_: Context, name: String) -> Result<()> {
		pop_macro(None, &name).map_err(|_| "error undefining macro".to_lua_err())
	}
	pub(crate) fn unregister(_: Context, arg: String) {
		todo!()
	}
	pub(crate) fn vercmp(_: Context, (s1, s2): (String, String)) {
		todo!()
	}
}

pub(crate) fn new() -> Result<Lua, rlua::Error> {
	let lua = Lua::new();
	lua.context(|ctx| -> rlua::Result<()> {
		let rpm = ctx.create_table()?;
		rpm.set("b64encode", ctx.create_function(lua_rpm::b64encode)?)?;
		rpm.set("b64decode", ctx.create_function(lua_rpm::b64decode)?)?;
		rpm.set("expand", ctx.create_function(lua_rpm::expand)?)?;
		rpm.set("define", ctx.create_function(lua_rpm::define)?)?;
		rpm.set("undefine", ctx.create_function(lua_rpm::undefine)?)?;
		rpm.set("isdefined", ctx.create_function(lua_rpm::isdefined)?)?;
		rpm.set("load", ctx.create_function(lua_rpm::load)?)?;
		// rpm.set("register", ctx.create_function(lua_rpm::register)?)?;
		// rpm.set("unregister", ctx.create_function(lua_rpm::unregister)?)?;
		// rpm.set("call", ctx.create_function(lua_rpm::call)?)?;
		// rpm.set("interactive", ctx.create_function(lua_rpm::interactive)?)?;
		// rpm.set("execute", ctx.create_function(lua_rpm::execute)?)?;
		// rpm.set("redirect2null", ctx.create_function(lua_rpm::redirect2null)?)?;
		// rpm.set("vercmp", ctx.create_function(lua_rpm::vercmp)?)?;
		// rpm.set("ver", ctx.create_function(lua_rpm::ver_new)?)?;
		// rpm.set("open", ctx.create_function(lua_rpm::open)?)?;
		// rpm.set("splitargs", ctx.create_function(lua_rpm::splitargs)?)?;
		// rpm.set("unsplitargs", ctx.create_function(lua_rpm::unsplitargs)?)?;
		Ok(())
	})?;
	Ok(lua)
}
