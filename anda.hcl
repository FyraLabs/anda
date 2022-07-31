project "anda" {
    pre_script {
        commands = ["echo 'hello'"]
    }
    /*

    script {
        stage "build" {
            commands = [
                "cargo build --release"
                ]
        }
        stage "test" {
            commands = [
                "cargo test --release"
                ]
        }
    }



    docker {
        dockerfile = "Dockerfile"
    }
    */

    script {
        stage "build" {
            commands = [
                "echo 'build command here'"
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