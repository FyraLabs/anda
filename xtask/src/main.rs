use anda::cli::Cli;
use anyhow::Result;
use clap::{Command, CommandFactory};
use clap_complete::{
    Generator,
    generate_to,
    shells::{Bash, Elvish, Fish, PowerShell, Shell, Zsh},
};
use std::env;
use std::fs::create_dir_all;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;

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
    )
}

fn gen_manpage(cmd: Rc<Command>, man_dir: &Path) {
    let name = cmd
        .get_display_name()
        .unwrap_or_else(|| cmd.get_name())
        .to_owned();
    if name.starts_with("anda-help") {
        return;
    }
    let mut out = File::create(man_dir.join(format!("{name}.1"))).unwrap();
    {
        let owned_name: &'static str = Box::leak(name.into_boxed_str());
        let man_cmd = (*cmd).clone().name(owned_name);
        clap_mangen::Man::new(man_cmd).render(&mut out).unwrap();
    }
    out.flush().unwrap();

    for sub in cmd.get_subcommands() {
        // let sub = sub.clone().display_name("anda-b");
        gen_manpage(Rc::new((*sub).clone()), man_dir)
    }
}

fn manpage() -> Result<()> {
    let app = Rc::new({
        let mut cmd = Cli::command();
        cmd.build();
        cmd
    });
    let out_dir = "target";
    let man_dir = PathBuf::from(&out_dir).join("man_pages");

    create_dir_all(&man_dir).unwrap();

    gen_manpage(app.clone(), &man_dir);

    let path = PathBuf::from(&out_dir).join("assets");

    let man_dir = path.join("man_pages");
    std::fs::create_dir_all(&man_dir).unwrap();
    gen_manpage(app, &man_dir);

    Ok(())
}

fn completion() -> Result<()> {
    let mut app = Cli::command();
    app.build();

    let out_dir = "target";
    let completion_dir = PathBuf::from(&out_dir).join("assets/completion");

    let shells: Vec<(Shell, & str)> = vec![
        (Shell::Bash, "bash"),
        (Shell::Fish, "fish"),
        (Shell::Zsh, "zsh"),
        (Shell::PowerShell, "pwsh"),
        (Shell::Elvish, "elvish"),
    ];

    for (shell, name) in shells {
        let dir = completion_dir.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        generate_to(shell, &mut app, "anda", dir)?;
    }

    Ok(())
}
