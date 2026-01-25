{
  description = "Otto - CLI tool for managing development workflows";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-parts.url = "github:hercules-ci/flake-parts";
    cargo2nix.url = "github:cargo2nix/cargo2nix/release-0.12";
    flake-utils.follows = "cargo2nix/flake-utils";
  };

  outputs = inputs @ {
    self,
    nixpkgs,
    rust-overlay,
    flake-parts,
    cargo2nix,
    ...
  }:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];

      perSystem = {
        pkgs,
        system,
        ...
      }: let
        overlays = [(import rust-overlay)];
        pkgsWithOverlays = import nixpkgs {
          inherit system overlays;
        };

        # Rust toolchain for development shell
        rustToolchain = pkgsWithOverlays.rust-bin.stable.latest.default.override {
          extensions = ["rust-src" "rust-analyzer"];
        };

        # Setup cargo2nix overlay for package building with rust-overlay
        pkgsWithCargo2nix = import nixpkgs {
          inherit system;
          overlays = [
            cargo2nix.overlays.default
            (import rust-overlay)
          ];
        };

        # Create the Rust package set from Cargo.nix
        rustPkgs = pkgsWithCargo2nix.rustBuilder.makePackageSet {
          packageFun = import ./Cargo.nix;
        };
      in {
        # Development shell (uses traditional rust-overlay)
        devShells.default = pkgsWithOverlays.mkShell {
          buildInputs = with pkgsWithOverlays; [
            rustToolchain
            cargo
            rustc
            pkg-config
          ];

          RUST_SRC_PATH = "${pkgsWithOverlays.rustPlatform.rustLibSrc}";
        };

        # Packages built with cargo2nix
        packages = rec {
          # The main otto binary
          otto = rustPkgs.workspace.otto {};
          default = otto;

          # Development shell from cargo2nix (includes all dependencies)
          devShell = rustPkgs.workspaceShell {};
        };

        # Default app
        apps.default = {
          type = "app";
          program = "${self.packages.${system}.otto}/bin/otto";
        };
      };
    };
}
