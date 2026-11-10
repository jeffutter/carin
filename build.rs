//! htmx's `htmx.min.js` is embedded the same way from `HTMX_JS` (src/ui.rs); unset, a
//! stub is written to OUT_DIR so plain `cargo build` works and the UI just has no
//! htmx (forms still work without it).
//!
//! Pele's `pele.global.js` is embedded with `include_str!(env!("PELE_JS"))`.
//!
//! Inside `nix develop` (and `nix build`) `PELE_JS` points at the real file.
//! When it is unset, a stub script is written to OUT_DIR instead so plain
//! `cargo build` / `cargo check` / rust-analyzer still work; Mermaid then
//! fails at runtime with a clear message, and `cfg(cairn_pele_stub)` is set so
//! tests that need real Pele can opt out. A set-but-bogus `PELE_JS` is a
//! misconfiguration and stays a hard error.

fn htmx() {
    println!("cargo:rerun-if-env-changed=HTMX_JS");
    match std::env::var("HTMX_JS") {
        Ok(p) if std::path::Path::new(&p).is_file() => {
            println!("cargo:rerun-if-changed={p}");
            println!("cargo:rustc-env=HTMX_JS={p}");
        }
        Ok(p) => panic!("HTMX_JS={p:?} is not a file; run inside `nix develop`"),
        Err(_) => {
            let out = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
            let stub = std::path::Path::new(&out).join("htmx.stub.js");
            std::fs::write(&stub, "/* htmx unavailable: built without HTMX_JS */\n")
                .expect("write htmx stub");
            println!("cargo:rustc-env=HTMX_JS={}", stub.display());
        }
    }
}

fn main() {
    htmx();
    println!("cargo:rerun-if-env-changed=PELE_JS");
    println!("cargo:rustc-check-cfg=cfg(cairn_pele_stub)");
    match std::env::var("PELE_JS") {
        Ok(p) if std::path::Path::new(&p).is_file() => {
            println!("cargo:rerun-if-changed={p}");
            println!("cargo:rustc-env=PELE_JS={p}");
        }
        Ok(p) => panic!("PELE_JS={p:?} is not a file; run inside `nix develop`"),
        Err(_) => {
            let out = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
            let stub = std::path::Path::new(&out).join("pele.stub.js");
            std::fs::write(
                &stub,
                "throw new Error(\"mermaid rendering unavailable: built without PELE_JS \
                 (build inside nix develop)\");\n",
            )
            .expect("write Pele stub");
            println!("cargo:rustc-env=PELE_JS={}", stub.display());
            println!("cargo:rustc-cfg=cairn_pele_stub");
            println!(
                "cargo:warning=PELE_JS is not set: mermaid diagrams are disabled in this build \
                 (use `nix develop` or `nix build` for a full build)"
            );
        }
    }
}
