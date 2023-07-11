use rlua::{Error, Lua, MultiValue};
use rustyline::Editor;

// https://github.com/amethyst/rlua/blob/master/examples/repl.rs
pub(crate) fn repl() {
	Lua::new().context(|lua| {
		let mut editor = Editor::<()>::new().expect("Can't make new rustyline::editor");

		loop {
			let mut prompt = "> ";
			let mut line = String::new();

			loop {
				match editor.readline(prompt) {
					Ok(input) => line.push_str(&input),
					Err(_) => return,
				}

				match lua.load(&line).eval::<MultiValue>() {
					Ok(values) => {
						editor.add_history_entry(line);
						println!("{}", values.iter().map(|value| format!("{:?}", value)).collect::<Vec<_>>().join("\t"));
						break;
					}
					Err(Error::SyntaxError { incomplete_input: true, .. }) => {
						// continue reading input and append it to `line`
						line.push('\n'); // separate input lines
						prompt = ">> ";
					}
					Err(e) => {
						eprintln!("error: {}", e);
						break;
					}
				}
			}
		}
	});
}
