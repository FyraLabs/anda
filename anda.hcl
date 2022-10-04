
// TODO: When hcl-rs finally finishes expression parsing, we can implement build script macros

project "test" {
    rpm {
        spec = "tests/umpkg.spec"
        pre_script = {
            commands = ["echo \"test\""]
        }
        post_script = {
            commands = [
                "ls",
            ]
        }
    }
    flatpak {
        manifest = "tests/org.flatpak.Hello.yml"
    }
}

project "anda" {
    rpm {
        spec = "rust-anda.spec"
    }
}

project "anda-git" {
    rpm {
        spec = "rust-anda-git.spec"
    }
}