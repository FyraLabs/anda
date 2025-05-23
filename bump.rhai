#!anda run

if labels.newver == () {
    print("please supply --labels newver=...");
    terminate();
}

let oldver = labels.get("oldver");
if oldver == () {
    oldver = find(`(?m)^version = "([\d.]+)"$`, open_file("anda-config/Cargo.toml", "r").read_string(), 1);
    print(`Assuming old version: ${oldver}`);
}
oldver.replace(".", "\\.");
labels.oldver = oldver;


fn edit_cargo_toml(path, labels) {
    print(`Editing file: ${path}`);
    let file = open_file(path);
    let s = sub(`version = "${labels.oldver}"`, `version = "${labels.newver}"`, file.read_string());
    file.seek(0);
    file.write(s);
}

edit_cargo_toml("anda-config/Cargo.toml", labels);
edit_cargo_toml("andax/Cargo.toml", labels);
edit_cargo_toml("Cargo.toml", labels);


print(`Editing file: rust-anda-git.spec`);
let spec1 = open_file("rust-anda-git.spec");
let s1 = sub(`(?m)^%global _version ${labels.oldver}$`, `%global _version ${labels.newver}`, spec1.read_string());
spec1.seek(0);
spec1.write(s1);


print(`Editing file: rust-anda.spec`);
let spec2 = open_file("rust-anda.spec");
let s2 = sub(`(?m)^(Version:\s+) ${labels.oldver}$`, `$1 ${labels.newver}`, spec2.read_string());
spec2.seek(0);
spec2.write(s2);
