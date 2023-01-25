
// Build macros are built using AndaX, a Rhai runtime for Andaman.

config {
    strip_prefix = "tests/"
}

project "test" {
        // pre_script = "tests/hello.sh"
    rpm {
        spec = "tests/umpkg.spec"
        // post_script = "tests/hello.sh"

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