
// TODO: When hcl-rs finally finishes expression parsing, we can implement build script macros

project "test" {
    rpm {
        spec = "tests/umpkg.spec"
        pre_script = {
            commands = ["ls -la /dev"]
        }
        post_script = {
            commands = [
                "ls -la",
                "tail anda-build/rpm/src/*.log"
            ]
        }
    }
    flatpak {
        manifest = "tests/org.flatpak.Hello.yml"
    }
}