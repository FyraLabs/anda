pub(crate) fn popen(cmd: &str) -> Option<String> {
	Some(
		String::from_utf8_lossy(
			&std::process::Command::new("sh")
				.args(["-c", cmd])
				.output()
				.ok()?
				.stdout,
		)
		.to_string(),
	)
}
/// dummy, please use [] instead.
pub(crate) fn rstrndup(s: &str) {}
