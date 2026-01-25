{
  description = "Otto - CLI tool for managing development workflows";

  inputs = {
    # Pin nixpkgs to 24.05 for compatibility with cargo2nix (with the new apple-sdk pattern while remaining compatible with cargo2nix)
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-parts.url = "github:hercules-ci/flake-parts";
    cargo2nix = {
      url = "github:cargo2nix/cargo2nix/release-0.12";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.follows = "cargo2nix/flake-utils";
  };

  outputs = inputs @ {
    self,
    nixpkgs,
    rust-overlay,
    flake-parts,
    cargo2nix,
    ...
  }: let
    # Overlay that can be used in other NixOS configurations
    ottoOverlay = final: prev: {
      otto = final.callPackage ./nix/package.nix {
        inherit cargo2nix nixpkgs;
        inherit (final.stdenv) system;
      };
    };
  in
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];

      perSystem = {
        config,
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
        # Additional packages can be added to the packages list below
        devShells.default = rustPkgs.workspaceShell {
          # Additional packages that should always be available
          # Extensible: add more packages here as needed
          packages = with pkgs; [
            # Core development tools
            pkg-config

            # Additional tools can be added here, e.g.:
            # tmux
            # just
            # jq
            # Note: prek is not available in all nixpkgs versions
          ];

          # Environment variables
          shellHook = ''
            # Welcome message
            echo "🔧 Otto development environment"
            echo "Rust version: $(rustc --version)"
            echo ""
            echo "Available commands:"
            echo "  cargo build    - Build the project"
            echo "  cargo test     - Run tests"
            echo "  cargo run --   - Run otto with arguments"
            echo ""
          '';
        };

        # Packages built with cargo2nix
        packages = rec {
          # The main otto binary
          otto = rustPkgs.workspace.otto {};
          default = otto;
        };

        # Default app
        apps.default = {
          type = "app";
          program = "${config.packages.default}/bin/otto";
        };
      };

      # Top-level outputs
      flake = {
        overlays.default = ottoOverlay;
      };
    };
}
