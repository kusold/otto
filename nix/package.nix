{
  cargo2nix,
  system,
  cargoNix ? ../Cargo.nix,
}:

let
  inherit (cargo2nix.inputs) nixpkgs rust-overlay;

  # Setup cargo2nix overlay for package building
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
    packageFun = import cargoNix;
  };

in
  rustPkgs.workspace.otto { }
