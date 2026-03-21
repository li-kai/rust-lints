{
  nixConfig = {
    extra-substituters = [ "https://li-kai.cachix.org" ];
    extra-trusted-public-keys = [
      "li-kai.cachix.org-1:hT/YtROuqsBhfSx1YDcMrFxBbnZLoyu+WA1CnhiUgWM="
    ];
  };

  description = "Development environment for rust-lints dylint library";

  inputs = {
    fenix = {
      url = "github:nix-community/fenix/monthly";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      fenix,
      crane,
    }:
    let
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      rustToolchainToml = builtins.fromTOML (builtins.readFile ./rust-toolchain);
      # Public compatibility metadata for downstream consumers.
      dylintVersion =
        let dep = cargoToml.dependencies.dylint_linting;
        in if builtins.isString dep then dep else dep.version;
      clippyRev = cargoToml.dependencies.clippy_utils.rev;
      mkSystem =
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          renderTemplate =
            path: replacements:
            builtins.replaceStrings
              (map (entry: entry.from) replacements)
              (map (entry: entry.to) replacements)
              (builtins.readFile path);

          rustManifestSha256 = "sha256-5XAIyRQMcynTWJvX5VkqErB0H4Oyg0AjeSefOyKSt7g=";

          # Parse channel from rust-toolchain to avoid duplicating the nightly date.
          toolchainChannel = rustToolchainToml.toolchain.channel;

          requiredRustComponents = [
            "cargo"
            "clippy"
            "rust-std"
            "rustc"
            "rustfmt"
            "rustc-dev"
            "llvm-tools-preview"
          ];

          mkRustToolchain =
            {
              extraRustComponents ? [ ],
              extraRustTargets ? [ ],
            }:
            let
              fenixPkgs = fenix.packages.${system};
              components = pkgs.lib.unique (requiredRustComponents ++ extraRustComponents);
              # Build the host toolchain first, then layer target stdlibs with `combine`.
              hostToolchain = (fenixPkgs.fromToolchainName {
                name = toolchainChannel;
                sha256 = rustManifestSha256;
              }).withComponents components;
              targetStdlibs = map (
                target:
                (fenixPkgs.targets.${target}.fromToolchainName {
                  name = toolchainChannel;
                  sha256 = rustManifestSha256;
                }).rust-std
              ) extraRustTargets;
            in
            fenixPkgs.combine ([ hostToolchain ] ++ targetStdlibs);

          rustToolchain = mkRustToolchain { };

          # Target triple for this system, used in RUSTUP_TOOLCHAIN
          targetTriple = pkgs.stdenv.hostPlatform.rust.rustcTarget;

          # RUSTUP_TOOLCHAIN must include the target triple for dylint's parse_toolchain()
          toolchainFull = "${toolchainChannel}-${targetTriple}";

          dylintMeta = {
            version = dylintVersion;
            toolchain = {
              channel = toolchainChannel;
              target = targetTriple;
              full = toolchainFull;
            };
          };

          # Shim that satisfies dylint's `rustup which <tool>` calls using
          # the nix-managed toolchain instead of a real rustup installation.
          rustupShim = pkgs.writeShellScriptBin "rustup" (
            renderTemplate ./nix/rustup-shim.sh [
              {
                from = "@TOOLCHAIN_FULL@";
                to = toolchainFull;
              }
            ]
          );

          # ---------------------------------------------------------------------------
          # Package build: dylib + driver for consumers
          # ---------------------------------------------------------------------------

          # Crane lib configured to use the fenix nightly toolchain.
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

          # Filtered source used by both vendoring and the cdylib build.
          cargoSrc = craneLib.cleanCargoSource ./.;

          darwinInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [ pkgs.libiconv ];

          # dylint-link: linker wrapper that tags output with @toolchain suffix.
          # Stable Rust suffices; no rustc_private APIs needed.
          dylintLink = pkgs.rustPlatform.buildRustPackage {
            pname = "dylint-link";
            version = dylintVersion;
            src = pkgs.fetchCrate {
              pname = "dylint-link";
              version = dylintVersion;
              hash = "sha256-TKjadUgjZ/ZqiTBctX6MoKlKUZL80wuMpG8r8n/sXmo=";
            };
            cargoHash = "sha256-FzpGao3jtZSLQ8iIXK8awM+BOtP32rAlJuKxwqv77Fg=";
          };

          # Wrapper that injects RUSTUP_TOOLCHAIN when the nix-built dylint-link runs.
          # Used in both the cdylib package build and the dev shell.
          dylintLinkWrapper = pkgs.writeShellScriptBin "dylint-link" ''
            export RUSTUP_TOOLCHAIN="''${RUSTUP_TOOLCHAIN:-${toolchainFull}}"
            exec ${dylintLink}/bin/dylint-link "$@"
          '';

          # Note: cargo-dylint is not packaged as a Nix derivation because its
          # transitive dep `dylint` has a build.rs that expects a `../driver`
          # sibling directory, which doesn't exist when building from crates.io.
          # Instead it's installed via `cargo install` in the shell hook.

          # Custom rustPlatform backed by the fenix nightly toolchain.
          # Required for dylint-driver, which links against rustc_driver from the sysroot.
          rustPlatformNightly = pkgs.makeRustPlatform {
            rustc = rustToolchain;
            cargo = rustToolchain;
          };

          # Prefetched clippy source at the commit matching our nightly toolchain.
          # The dylint_driver build script clones rust-clippy to extract extra
          # symbols from clippy_utils/src/sym.rs. Since Nix builds have no network
          # access, we prefetch the source and provide a git wrapper that serves
          # it locally. The rev is derived from Cargo.toml so there's one source of truth.
          clippySrc = pkgs.fetchFromGitHub {
            owner = "rust-lang";
            repo = "rust-clippy";
            rev = clippyRev;
            hash = "sha256-TkpjcIp+lQcIfm93bZKMCz4+CDY2/0j7HBmsI1uEgsQ=";
          };

          # Git wrapper that intercepts clone/checkout of rust-clippy and serves
          # from the prefetched source instead. All other git operations pass through.
          gitClippyWrapper = pkgs.writeShellScriptBin "git" (
            renderTemplate ./nix/git-clippy-wrapper.sh [
              {
                from = "@GREP@";
                to = "${pkgs.gnugrep}/bin/grep";
              }
              {
                from = "@REAL_GIT@";
                to = "${pkgs.git}/bin/git";
              }
              {
                from = "@CLIPPY_SRC@";
                to = toString clippySrc;
              }
              {
                from = "@TOOLCHAIN_CHANNEL@";
                to = toolchainChannel;
              }
            ]
          );

          # dylint-driver: the rustc driver binary shipped to consumers.
          # The binary doesn't exist in any published crate — cargo-dylint synthesizes
          # it at runtime from a generated project that wraps the dylint_driver library.
          # We replicate that here with nix/dylint-driver/.
          # The -rpath bakes in the sysroot so consumers don't need the nightly
          # toolchain locally. The rustToolchain store path becomes a closure
          # dependency automatically, so Nix pulls it from fenix.cachix.org.
          dylintDriver = rustPlatformNightly.buildRustPackage {
            pname = "dylint-driver";
            version = dylintVersion;
            src = ./nix/dylint-driver;
            cargoLock.lockFile = ./nix/dylint-driver/Cargo.lock;
            nativeBuildInputs = [ pkgs.pkg-config rustupShim gitClippyWrapper ];
            buildInputs = [ pkgs.openssl pkgs.zlib ] ++ darwinInputs;
            RUSTUP_TOOLCHAIN = toolchainFull;
            RUSTFLAGS =
              if pkgs.stdenv.isDarwin
              then "-C link-arg=-rpath -C link-arg=${rustToolchain}/lib"
              else "-C link-arg=-Wl,-rpath,${rustToolchain}/lib";
            meta.mainProgram = "dylint-driver";
          };

          # Vendor deps with a workaround for the clippy git dep: its test crates
          # reference a README.md that doesn't exist, which breaks `cargo package`
          # during crane's vendoring step.
          cargoVendorDir = craneLib.vendorCargoDeps {
            src = cargoSrc;
            overrideVendorGitCheckout = _ps: drv:
              drv.overrideAttrs (_old: {
                postPatch = (_old.postPatch or "") + ''
                  for dir in tests/ui-cargo/cargo_common_metadata/pass \
                             tests/ui-cargo/cargo_common_metadata/fail; do
                    if [ -d "$dir" ] && [ -f "$dir/Cargo.toml" ] && [ ! -f "$dir/README.md" ]; then
                      touch "$dir/README.md"
                    fi
                  done
                '';
              });
          };

          # Build rust-lints as a cdylib using crane + the nix-built dylint-link.
          # .cargo/config.toml sets linker = "dylint-link" per target triple, which
          # causes dylint-link to tag the output filename with @toolchain.
          #
          # Note: crane's buildDepsOnly is intentionally not used here.
          # The vendored `dylint` crate (transitive dev-dep via dylint_testing)
          # has a build.rs that expects a `../driver` sibling directory, which
          # doesn't exist in crane's isolated vendor layout. Patching the vendor
          # dir to add a stub works but adds complexity for little gain — we
          # publish infrequently and cachix handles consumer caching.
          # This matches the approach used by TheNeikos/nix-dylint.
          rustLintsLib = craneLib.buildPackage {
            pname = "rust-lints";
            src = cargoSrc;
            inherit cargoVendorDir;
            cargoArtifacts = null;
            cargoExtraArgs = "--lib";

            nativeBuildInputs = [ dylintLinkWrapper rustupShim ];
            buildInputs = [ pkgs.zlib ] ++ darwinInputs;

            RUSTUP_TOOLCHAIN = toolchainFull;

            # cdylib targets don't produce testable artifacts; skip cargo test.
            doCheck = false;

            # cargo install doesn't work for cdylib targets; copy the tagged dylib directly.
            installPhase = ''
              runHook preInstall
              mkdir -p $out/lib
              # Assert that dylint-link tagged the dylib with @toolchain in the filename.
              if ! ls target/release/librust_lints@*.* 1>/dev/null 2>&1; then
                echo "ERROR: No @toolchain-tagged dylib found in target/release/" >&2
                echo "Expected: librust_lints@${toolchainFull}.<ext>" >&2
                echo "Found:" >&2
                ls -la target/release/librust_lints* 2>&1 >&2 || true
                exit 1
              fi
              cp target/release/librust_lints@*.* $out/lib/
              runHook postInstall
            '';
          };

          # Combined package exposing both artifacts at the paths dylint expects:
          #   $out/lib/librust_lints@<toolchain>.<ext>    → DYLINT_LIBRARY_PATH
          #   $out/drivers/<toolchain>/dylint-driver       → DYLINT_DRIVER_PATH
          rustLints = pkgs.symlinkJoin {
            name = "rust-lints";
            paths = [ rustLintsLib ];
            postBuild = ''
              mkdir -p $out/drivers/${toolchainFull}
              cp ${dylintDriver}/bin/dylint-driver $out/drivers/${toolchainFull}/
            '';
          };

          mkDevShell =
            {
              pkgs ? nixpkgs.legacyPackages.${system},
              packages ? [ ],
              extraRustComponents ? [ ],
              extraRustTargets ? [ ],
              shellHook ? "",
              extraEnv ? { },
            }:
            let
              shellRustToolchain = mkRustToolchain {
                inherit extraRustComponents extraRustTargets;
              };
            in
            pkgs.mkShell (
              extraEnv
              // {
                name = "rust-lints-dev-environment";
                packages = [
                  shellRustToolchain
                  rustupShim
                ] ++ packages;
                DYLINT_LIBRARY_PATH = "${rustLints}/lib";
                DYLINT_DRIVER_PATH = "${rustLints}/drivers";
                RUSTUP_TOOLCHAIN = toolchainFull;
                shellHook =
                  renderTemplate ./nix/cargo-dylint-shell-hook.sh [
                    {
                      from = "@RUST_TOOLCHAIN_BIN@";
                      to = "${shellRustToolchain}/bin";
                    }
                    {
                      from = "@RUSTUP_SHIM_BIN@";
                      to = "${rustupShim}/bin";
                    }
                    {
                      from = "@RUSTC_BIN@";
                      to = "${shellRustToolchain}/bin/rustc";
                    }
                    {
                      from = "@CARGO_BIN@";
                      to = "${shellRustToolchain}/bin/cargo";
                    }
                    {
                      from = "@DYLINT_VERSION@";
                      to = dylintVersion;
                    }
                  ]
                + pkgs.lib.optionalString (shellHook != "") "\n${shellHook}\n";
              }
            );
        in
        {
          inherit dylintMeta mkDevShell rustLints;

          outputs = {
            packages.default = rustLints;

            devShells.default = mkDevShell {
              packages = [
                # Rust development tools
                pkgs.cargo-watch # Auto-rebuild on file changes
                pkgs.cargo-edit # cargo add/rm/upgrade commands
                pkgs.cargo-audit # Security vulnerability scanning
                fenix.packages.${system}.rust-analyzer # Rust language server
                pkgs.just # Command runner

                # Development utilities
                pkgs.git
                pkgs.zlib

                # Wrapper for dylint-link that injects RUSTUP_TOOLCHAIN.
                # Needed when building this repo from source in the dev shell.
                dylintLinkWrapper
              ] ++ darwinInputs;
              extraEnv = {
                RUST_BACKTRACE = "1";
              };
            };

            checks.dylint-version-compat = pkgs.runCommand "dylint-version-compat" { } ''
              [ "${dylintVersion}" = "${dylintMeta.version}" ]
              touch "$out"
            '';
          };
        };
    in
    flake-utils.lib.eachDefaultSystem (system: (mkSystem system).outputs)
    // {
      lib = {
        dylintVersion = dylintVersion;
        dylint = {
          version = dylintVersion;
          forSystem = system: (mkSystem system).dylintMeta;
        };
        mkDevShell =
          {
            pkgs,
            packages ? [ ],
            extraRustComponents ? [ ],
            extraRustTargets ? [ ],
            shellHook ? "",
            extraEnv ? { },
          }:
          (mkSystem pkgs.system).mkDevShell {
            inherit
              pkgs
              packages
              extraRustComponents
              extraRustTargets
              shellHook
              extraEnv
              ;
          };
      };
    };
}
