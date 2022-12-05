// let req = new_req("https://api.snapcraft.io/v2/snaps/info/authy");
// req.head("Snap-Device-Series", "16");
// req.head("User-Agent", USER_AGENT);
// let obj = json(req.get()) @ "channel-map" @ 0 @ "version";
// rpm.version(obj.str());
// rpm.f = sub("2.2.2", "3.3.3", rpm.f);

//rpm.version(gh("Bonandry/adwaita-plus"));
// print(env("PATH"));
rpm.version("0.3.63");
rpm.source(0, "https://github.com/Ultramarine-Linux/umpkg/archive/refs/tags/%{version}.tar.gz");
