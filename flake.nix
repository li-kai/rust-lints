{
  description = "Development environment for rust-lints dylint library";

  inputs = {
    fenix = {
      url = "github:nix-community/fenix/monthly";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      fenix,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        # Nightly toolchain pinned via rust-toolchain; rustc-dev required by dylint
        rustToolchain = fenix.packages.${system}.fromToolchainFile {
          file = ./rust-toolchain;
          sha256 = "sha256-5XAIyRQMcynTWJvX5VkqErB0H4Oyg0AjeSefOyKSt7g=";
        };

        # Target triple for this system, used in RUSTUP_TOOLCHAIN
        targetTriple = pkgs.stdenv.hostPlatform.rust.rustcTarget;

        # Parse channel from rust-toolchain to avoid duplicating the nightly date
        toolchainChannel = (builtins.fromTOML (builtins.readFile ./rust-toolchain)).toolchain.channel;

        # RUSTUP_TOOLCHAIN must include the target triple for dylint's parse_toolchain()
        toolchainFull = "${toolchainChannel}-${targetTriple}";

        # Version of dylint tools to install (derived from Cargo.toml to stay in sync)
        dylintVersion =
          let dep = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).dependencies.dylint_linting;
          in if builtins.isString dep then dep else dep.version;

        # Name of the real dylint-link binary (installed by cargo, renamed so our wrapper takes precedence)
        dylintLinkReal = ".dylint-link-real";

        # Shim that satisfies dylint's `rustup which <tool>` calls using
        # the nix-managed toolchain instead of a real rustup installation.
        rustupShim = pkgs.writeShellScriptBin "rustup" ''
          case "$1" in
            which)
              exec which "$2"
              ;;
            show)
              echo "nix-managed: $(rustc --version)"
              ;;
            toolchain)
              echo "${toolchainFull} (nix-managed)"
              ;;
            run)
              # `rustup run <toolchain> <cmd> [args...]` — skip toolchain arg, run cmd
              if [ $# -lt 3 ]; then
                echo "rustup shim: 'run' requires a toolchain and command" >&2
                exit 1
              fi
              shift 2
              exec "$@"
              ;;
            *)
              echo "rustup shim: unsupported command '$*'" >&2
              exit 1
              ;;
          esac
        '';

        # Wrapper for dylint-link that always injects RUSTUP_TOOLCHAIN,
        # even when dylint's sanitize_environment() strips it.
        dylintLinkWrapper = pkgs.writeShellScriptBin "dylint-link" ''
          export RUSTUP_TOOLCHAIN="''${RUSTUP_TOOLCHAIN:-${toolchainFull}}"
          exec "$HOME/.cargo/bin/${dylintLinkReal}" "$@"
        '';

        # Cargo wrapper that re-injects RUSTUP_TOOLCHAIN after dylint strips it.
        # dylint's sanitize_environment() calls env_remove("RUSTUP_TOOLCHAIN")
        # on subprocess commands. With real rustup, cargo is a proxy that reads
        # rust-toolchain files and re-sets the env var. This wrapper emulates
        # that behavior for nix-managed toolchains.
        cargoWrapper = pkgs.writeShellScriptBin "cargo" ''
          export RUSTUP_TOOLCHAIN="''${RUSTUP_TOOLCHAIN:-${toolchainFull}}"
          exec "${rustToolchain}/bin/cargo" "$@"
        '';
      in
      {
        devShells.default = pkgs.mkShell {
          name = "dev-environment";

          packages = [
            # Cargo wrapper that ensures RUSTUP_TOOLCHAIN survives dylint's
            # sanitize_environment(). Must appear before rustToolchain in PATH.
            cargoWrapper

            # Rust nightly toolchain with rustc-dev and llvm-tools-preview
            rustToolchain

            # Rust development tools
            pkgs.cargo-watch # Auto-rebuild on file changes
            pkgs.cargo-edit # cargo add/rm/upgrade commands
            pkgs.cargo-audit # Security vulnerability scanning
            pkgs.rust-analyzer # Rust language server
            pkgs.just # Command runner

            # Development utilities
            pkgs.git
            pkgs.zlib

            # dylint calls `rustup which rustc` to locate the toolchain;
            # this shim redirects to the nix-managed binaries.
            rustupShim

            # Wrapper for dylint-link that injects RUSTUP_TOOLCHAIN
            dylintLinkWrapper
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.libiconv
          ];

          # Environment variables
          RUST_BACKTRACE = "1";
          # dylint-link requires RUSTUP_TOOLCHAIN even when using fenix/nix-managed toolchains
          RUSTUP_TOOLCHAIN = toolchainFull;

          shellHook = ''
            echo "Rust toolchain: $(rustc --version)"

            # Install dylint-link if missing or version has changed.
            # Uses the real cargo binary directly to avoid the wrapper.
            _dylint_marker="$HOME/.cargo/bin/.dylint-link-version"
            if [ "$(cat "$_dylint_marker" 2>/dev/null)" != "${dylintVersion}" ]; then
              echo "Installing dylint-link v${dylintVersion}..."
              "${rustToolchain}/bin/cargo" install dylint-link --version "${dylintVersion}" --quiet
              mv "$HOME/.cargo/bin/dylint-link" "$HOME/.cargo/bin/${dylintLinkReal}"
              echo "${dylintVersion}" > "$_dylint_marker"
            fi

            # Install cargo-dylint if missing or version has changed.
            _cargo_dylint_marker="$HOME/.cargo/bin/.cargo-dylint-version"
            if [ "$(cat "$_cargo_dylint_marker" 2>/dev/null)" != "${dylintVersion}" ]; then
              echo "Installing cargo-dylint v${dylintVersion}..."
              "${rustToolchain}/bin/cargo" install cargo-dylint --version "${dylintVersion}" --quiet
              echo "${dylintVersion}" > "$_cargo_dylint_marker"
            fi
          '';
        };
      }
    );
}
