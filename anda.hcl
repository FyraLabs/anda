project "anda" {
    pre_script {
        commands = ["echo 'hello'"]
    }

    rpmbuild {
        mode = "cargo"
        package = "anda"
        build_deps = ["openssl-devel", "rust-packaging"]
    }
    post_script {
        commands = [
            "echo 'world'"
        ]
    }

    env = {
        TEST = "test"
    }

    // if scripts are defined and type is docker or rpm, the scripts will be executed
    // before the package is built.
}


// TODO: When hcl-rs finally finishes expression parsing, we can implement build script macros

project "test" {
    rpm {
        spec = "anda/tests/umpkg.spec"
        pre_script = {
            commands = ["ls -la /dev",

            ]
        }
        post_script = {
            commands = ["ls -la",
            "tail anda-build/rpm/src/*.log"
            ]
        }
    }
}