# PROTOTYPE spike, wipe me. Does rquickjs + Pele build under nix, and what does a render cost?
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    crane.url = "github:ipetkov/crane";
  };
  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; overlays = [ (import rust-overlay) ]; };
        craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-bin.stable.latest.default;
        pele = pkgs.fetchzip {
          url = "https://registry.npmjs.org/pele/-/pele-1.13.0.tgz";
          hash = "sha256-1mxtui9HY6TO5Sdpt+DtoTf/O7q8rvcba76Ifg1txBs=";
        };
        common = {
          src = craneLib.cleanCargoSource ./.;
          nativeBuildInputs = [ pkgs.clang ];
          PELE_JS = "${pele}/dist/pele.global.js";
        };
        deps = craneLib.buildDepsOnly common;
      in {
        packages.default = craneLib.buildPackage (common // { cargoArtifacts = deps; });
      });
}
