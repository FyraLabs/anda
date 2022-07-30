project "anda" {
    proj_type = "rpm" // rpm, generic, docker

    // spec file for rpm
    spec = "./anda.spec"
    // Docker images
    // dockerfile = "./Dockerfile"
    /*
    scripts = {
        script "build" {
            command = ["cargo", "build", "--release"]
        }
        script "test" {
            command = ["cargo", "test", "--release"]
        }
        script "install" {
            command = ["cargo", "install", "."]
        }
    }
    */

    // if scripts is defined and type is docker or rpm, the scripts will be executed
    // before the package is built.
}