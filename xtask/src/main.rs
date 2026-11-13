use anda::cli::Cli;
use anyhow::Result;
use clap::{Command, CommandFactory};
use clap_complete::{generate_to, shells::Shell};
use std::env;
use std::fs::create_dir_all;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let task = env::args().nth(1);
    match task.as_deref() {
        Some("manpage") => manpage()?,
        Some("completion") => completion()?,
        _ => print_help(),
    }
    Ok(())
}

fn print_help() {
    eprintln!(
        "Tasks:
manpage            builds application and man pages
completion         builds shell completions
"
    );
}

fn gen_manpage(cmd: &Command, man_dir: &Path) -> Result<()> {
    let name = cmd.get_display_name().unwrap_or_else(|| cmd.get_name()).to_owned();
    if name.starts_with("anda-help") {
        return Ok(());
    }
    let mut out = File::create(man_dir.join(format!("{name}.1")))?;
    let name: &'static str = Box::leak(name.into_boxed_str());
    clap_mangen::Man::new(cmd.clone().name(name)).render(&mut out)?;
    out.flush()?;

    for sub in cmd.get_subcommands() {
        gen_manpage(sub, man_dir)?;
    }
    Ok(())
}

fn manpage() -> Result<()> {
    let mut app = Cli::command();
    app.build();
    let out_dir = PathBuf::from("target");

    for man_dir in [out_dir.join("man_pages"), out_dir.join("assets/man_pages")] {
        create_dir_all(&man_dir)?;
        gen_manpage(&app, &man_dir)?;
    }

    Ok(())
}

fn completion() -> Result<()> {
    let mut app = Cli::command();
    app.build();

    let out_dir = "target";
    let completion_dir = PathBuf::from(&out_dir).join("assets/completion");

    let shells: Vec<(Shell, &str)> = vec![
        (Shell::Bash, "bash"),
        (Shell::Fish, "fish"),
        (Shell::Zsh, "zsh"),
        (Shell::PowerShell, "pwsh"),
        (Shell::Elvish, "elvish"),
    ];

    for (shell, name) in shells {
        let dir = completion_dir.join(name);
        create_dir_all(&dir)?;
        generate_to(shell, &mut app, "anda", dir)?;
    }

    Ok(())
}
