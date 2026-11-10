{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    { self
    , nixpkgs
    , flake-utils
    , rust-overlay
    , crane
    ,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        lib = nixpkgs.lib;

        # One rust-overlay toolchain for rustc, cargo, rustfmt and clippy.
        rustToolchain = pkgs.rust-bin.stable.latest.default;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        src = lib.cleanSourceWith { src = craneLib.path ./.; };

        envVars = lib.attrsets.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          RUSTFLAGS = "-Clinker=clang -Clink-arg=--ld-path=${pkgs.mold}/bin/mold";
        };

        # Pele, the Mermaid renderer, embedded with include_str! (src/diagram.rs).
        pele = pkgs.fetchzip {
          url = "https://registry.npmjs.org/pele/-/pele-1.13.0.tgz";
          hash = "sha256-1mxtui9HY6TO5Sdpt+DtoTf/O7q8rvcba76Ifg1txBs=";
        };
        peleJs = "${pele}/dist/pele.global.js";

        commonArgs = {
          inherit src;
          PELE_JS = peleJs;
          strictDeps = true;
          nativeBuildInputs = [ pkgs.clang ];
          buildInputs = lib.optionals pkgs.stdenv.hostPlatform.isDarwin [ pkgs.libiconv ];
        } // envVars;

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        cairn = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          doCheck = false; # nextest runs in checks.nextest
        });
      in
      {
        packages.default = cairn;

        checks = {
          build = cairn;
          clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- -D warnings";
          });
          fmt = craneLib.cargoFmt { inherit src; };
          nextest = craneLib.cargoNextest (commonArgs // {
            inherit cargoArtifacts;
            # The scoping tests shell out to git.
            nativeCheckInputs = [ pkgs.git ];
          });
        };

        devShells.default = pkgs.mkShell ({
          packages = [
            rustToolchain
            pkgs.cargo-nextest
            pkgs.cargo-watch
            pkgs.clang
            pkgs.sqlite
          ] ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.mold ];
          PELE_JS = peleJs;
        } // envVars);

        formatter = pkgs.nixpkgs-fmt;
      }
    );
}
