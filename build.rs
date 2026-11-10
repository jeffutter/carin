//! Pele's `pele.global.js` is embedded with `include_str!(env!("PELE_JS"))`.
//! Fail early with a pointer to the nix devShell, which exports it.

fn main() {
    println!("cargo:rerun-if-env-changed=PELE_JS");
    match std::env::var("PELE_JS") {
        Ok(p) if std::path::Path::new(&p).is_file() => {
            println!("cargo:rerun-if-changed={p}");
        }
        Ok(p) => panic!("PELE_JS={p:?} is not a file; run inside `nix develop`"),
        Err(_) => panic!(
            "PELE_JS is not set: it must point at Pele's pele.global.js. \
             Run cargo inside `nix develop` (the devShell exports it)."
        ),
    }
}
