// let req = new_req("https://api.snapcraft.io/v2/snaps/info/authy");
// req.head("Snap-Device-Series", "16");
// req.head("User-Agent", USER_AGENT);
// let obj = json(req.get()) @ "channel-map" @ 0 @ "version";
// rpm.version(obj.str());
// rpm.f = sub("2.2.2", "3.3.3", rpm.f);

// print(gh("Bonandry/adwaita-plus"));
print(env("PATH"));

// rpm.version("0.3.63");
// rpm.source(0, "https://github.com/Ultramarine-Linux/umpkg/archive/refs/tags/%{version}.tar.gz");

// let manifest = anda::cfg::load_file("/stuff/chubby/terra/anda.hcl");
// print(manifest);

// print(npm("discord.js"));

let obj = #{
    "a": 1,
    "b": 2 ,
    "c": #{
        "d": "e",
    },
    "bar": npm("discord.js")
};

// print(obj);
print(obj);
// funny template test
let t = template_file(obj,"tests/umpkg.spec.in");

print(t);