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

        commonArgs = {
          inherit src;
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
        } // envVars);

        formatter = pkgs.nixpkgs-fmt;
      }
    );
}
