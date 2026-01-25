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
          rustChannel = "stable";
          rustVersion = "latest";
          packageFun = import ./Cargo.nix;
        };
      in {
        # Development shell using cargo2nix (includes all dependencies)
        devShells.default = rustPkgs.workspaceShell {};

        # Packages built with cargo2nix
        packages = rec {
          # The main otto binary
          otto = rustPkgs.workspace.otto {};
          default = otto;
        };

        # Default app
        apps.default = {
          type = "app";
          program = "${self.packages.${system}.otto}/bin/otto";
        };
      };
    };
}
