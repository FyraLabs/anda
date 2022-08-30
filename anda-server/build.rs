use std::{env, fs, process::Command};
// use std::env;
// use std::path::PathBuf;
use anyhow::Result;
use shells::sh;
// check if in container
use in_container::in_container;

fn pnpm() -> Result<()> {
    Command::new("pnpm").arg("install").status()?;
    Ok(())
}

fn main() {
    println!("cargo:rerun-if-changed=anda-frontend/src");

    let old_pwd = std::env::current_dir().unwrap();
    // change current directory to anda-frontend
    std::env::set_current_dir("anda-frontend").unwrap();
    pnpm().unwrap();

    if pnpm().is_err() && in_container() && is_root::is_root() {
        // check if node is installed
        if Command::new("node").status().unwrap().success() {
            // This is not a good way to install pnpm, since it causes side effects.
            // would be better if we added prebuild script to anda config.
            sh!("curl -f https://get.pnpm.io/v6.16.js | node - add --global pnpm@7");
            // try to run again
            pnpm().unwrap();
        } else {
            panic!("node is not installed");
        }
        //panic!("pnpm is not installed, and not in a build container! install pnpm and try again");
    }

    Command::new("pnpm")
        .arg("build")
        .arg("--outDir")
        .arg(format!("{}/web", env::var("OUT_DIR").unwrap()))
        .status()
        .unwrap();
    std::env::set_current_dir(old_pwd).unwrap();

    // if symlink already exists
    let symlink = fs::read_link("dist");
    /* if symlink.is_ok() {
        // check if symlink is correct
        let symlink_path = symlink.unwrap();
        if symlink_path.to_str().unwrap() == "../anda-frontend/dist" {
            println!("symlink already exists");
            return;
        } else {
            std::os::unix::fs::symlink("../anda-frontend/dist", "dist").unwrap();
        }
    } else {
        std::os::unix::fs::symlink("../anda-frontend/dist", "dist").unwrap();
    } */

    // out dir
    let out_dir = std::env::var("OUT_DIR").unwrap();

    if let Ok(symlink_path) = symlink {
        if symlink_path.to_str().unwrap() == "anda-frontend/dist" {
            println!("symlink already exists");
        } else {
            std::os::unix::fs::symlink("anda-frontend/dist", format!("{}/dist", out_dir)).unwrap();
        }
    }

    // copy anda-frontend/dist folder to anda-server/dist folder
    //std::os::unix::fs::symlink("../anda-frontend/dist", "dist").unwrap();

    // println!("{:?}", exit_status);
}
