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

        # htmx, embedded with include_str! (src/ui.rs) and served from the UI port.
        htmxJs = pkgs.fetchurl {
          url = "https://cdn.jsdelivr.net/npm/htmx.org@2.0.4/dist/htmx.min.js";
          hash = "sha256-4gndpcgjVHnzFm3vx3UOHbzVpcGAi3eS/C5nM3aPtEc=";
        };

        commonArgs = {
          inherit src;
          PELE_JS = peleJs;
          HTMX_JS = htmxJs;
          strictDeps = true;
          nativeBuildInputs = [ pkgs.clang ];
          buildInputs = lib.optionals pkgs.stdenv.hostPlatform.isDarwin [ pkgs.libiconv ];
        } // envVars;

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        cairn = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          doCheck = false; # nextest runs in checks.nextest
        });
        # The agent skill: a directory holding SKILL.md, so another flake can
        # link it as a whole skill directory (e.g. ~/.claude/skills/cairn).
        skill = pkgs.runCommand "cairn-skill" { nativeBuildInputs = [ cairn ]; } ''
          mkdir -p $out
          cairn skill > $out/SKILL.md
          cairn skill --check $out/SKILL.md
        '';
        # The Herdr mod (herdr/cairn/mods/cairn), laid out as a Herdr home's mods/ folder so
        # another flake can link $out/mods/cairn into ~/.config/herdr-app/mods/. The mod's
        # cairn binary is pinned to this flake's package.
        herdr-mod = pkgs.runCommand "cairn-herdr-mod" { } ''
          mkdir -p $out/mods
          cp -r ${./herdr/cairn/mods/cairn} $out/mods/cairn
          chmod -R u+w $out/mods/cairn
          substituteInPlace $out/mods/cairn/main.js --replace-fail "@cairn@" "${cairn}/bin/cairn"
        '';
      in
      {
        packages = {
          default = cairn;
          inherit skill herdr-mod;
        };

        checks = {
          build = cairn;
          inherit herdr-mod;
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
            pkgs.cargo-release
            pkgs.cargo-watch
            pkgs.clang
            pkgs.sqlite
          ] ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.mold ];
          PELE_JS = peleJs;
          HTMX_JS = htmxJs;
        } // envVars);

        formatter = pkgs.nixpkgs-fmt;
      }
    );
}
