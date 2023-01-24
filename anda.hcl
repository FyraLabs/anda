
// TODO: When hcl-rs finally finishes expression parsing, we can implement build script macros

config {
    strip_prefix = "tests/"
}

project "test" {
        pre_script = "tests/hello.sh"
    rpm {
        spec = "tests/umpkg.spec"
        post_script = "tests/hello.sh"

        sources = "tests/"
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