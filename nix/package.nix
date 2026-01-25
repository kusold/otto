{
  cargo2nix,
  nixpkgs,
  system,
  cargoNix ? ../Cargo.nix,
}:

let
  inherit (cargo2nix.inputs) rust-overlay;

  # Setup cargo2nix overlay for package building
  # Use the main nixpkgs input instead of cargo2nix's internal nixpkgs
  # to avoid deprecated darwin SDK references (fixes "darwin.apple_sdk_11_0 has been removed")
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
