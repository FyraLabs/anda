// we include the CLI module so we can generate
include!("src/cli.rs");
use std::fs::create_dir_all;
use std::fs::File;
use std::io::Write;
use std::path::Path;
fn main() {
    let mut app = Cli::command();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let man_dir = PathBuf::from(&out_dir).join("man_pages");

    create_dir_all(&man_dir).unwrap();

    fn gen_manpage(cmd: &clap::Command, man_dir: &Path) {
        let name = cmd.get_display_name().unwrap_or_else(|| cmd.get_name());
        let mut out = File::create(man_dir.join(format!("{name}.1"))).unwrap();
        clap_mangen::Man::new(cmd.clone().display_name(name).name(name))
            .render(&mut out)
            .unwrap();
        out.flush().unwrap();

        for sub in cmd.get_subcommands() {
            // let sub = sub.clone().display_name("anda-b");
            gen_manpage(sub, man_dir)
        }
    }

    app.build();

    gen_manpage(&app, &man_dir);

    let path = PathBuf::from(&out_dir)
        .ancestors()
        .nth(4)
        .unwrap()
        .join("assets");

    let man_dir = path.join("man_pages");
    std::fs::create_dir_all(&man_dir).unwrap();
    gen_manpage(&app, &man_dir);
}
