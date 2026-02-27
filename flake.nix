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
          sha256 = pkgs.lib.fakeSha256;
        };
      in
      {
        devShells.default = pkgs.mkShell {
          name = "dev-environment";

          packages = [
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

            # dylint-link must be installed separately:
            #   cargo install dylint-link
          ];

          # Environment variables
          RUST_BACKTRACE = "1";

          shellHook = ''
            echo "Rust toolchain: $(rustc --version)"
            if ! command -v dylint-link &>/dev/null; then
              echo "Note: dylint-link not found. Install with: cargo install dylint-link"
            fi
          '';
        };
      }
    );
}
