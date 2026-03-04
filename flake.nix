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
        toolchainChannel = "nightly-2026-01-22";
        # RUSTUP_TOOLCHAIN must include the target triple for dylint's parse_toolchain()
        toolchainFull = "nightly-2026-01-22-${
          if system == "aarch64-darwin" then "aarch64-apple-darwin"
          else if system == "x86_64-darwin" then "x86_64-apple-darwin"
          else if system == "x86_64-linux" then "x86_64-unknown-linux-gnu"
          else if system == "aarch64-linux" then "aarch64-unknown-linux-gnu"
          else throw "unsupported system: ${system}"
        }";

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
          exec "$HOME/.cargo/bin/.dylint-link-real" "$@"
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
          ];

          # Environment variables
          RUST_BACKTRACE = "1";
          # dylint-link requires RUSTUP_TOOLCHAIN even when using fenix/nix-managed toolchains
          RUSTUP_TOOLCHAIN = toolchainFull;

          shellHook = ''
            echo "Rust toolchain: $(rustc --version)"
            # Install the real dylint-link binary, renamed so our wrapper can call it
            if [ ! -f "$HOME/.cargo/bin/.dylint-link-real" ]; then
              echo "Installing dylint-link..."
              cargo install dylint-link --quiet
              mv "$HOME/.cargo/bin/dylint-link" "$HOME/.cargo/bin/.dylint-link-real"
            fi
          '';
        };
      }
    );
}
