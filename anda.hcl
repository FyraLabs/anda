project "anda" {
    pre_script {
        commands = ["echo 'hello'"]
    }
    /*

    script {
        stage "build" {
            depends = ["prepare"]
            commands = [
                "cargo build --release"
                ]
        }
        stage "test" {
            depends = ["build"]
            commands = [
                "cargo test --release"
                ]
        }
    }



    docker {
        image "anda/anda" {
            version = "latest"
            dir = "."
        }
    }


    rollback {
        stage "build" {
            commands = [
                "echo 'rollback'"
            ]

        }
    }

    */

    script {
        stage "build" {
            commands = [
                "echo 'build command here'",
                "echo $TEST"
            ]
        }

        stage "test" {
            depends = ["build"]
            commands = [
                "echo 'test command here'",
                "echo $TEST"
            ]
        }
    }

    /* docker {
        image "anda/anda" {
            tag_latest = true
            version = "$COMMIT_ID"
            workdir = "."
        }
        image "test" {
            tag_latest = true
            version = "$COMMIT_ID"
            workdir = "."
        }
    } */

    rollback {
        stage "build" {
            commands = [
                "echo 'rollback command here'"
            ]
        }
    }

    rpmbuild {
        spec = "./anda.spec"
    }
    post_script {
        commands = [
            "echo 'world'"
        ]
    }


    // if scripts are defined and type is docker or rpm, the scripts will be executed
    // before the package is built.
}

project "z" {
    script {
        stage "build" {
            commands = [
                "echo 'build command here'"
            ]
        }
    }
}