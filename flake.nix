{
  description = "cli development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [
          (import rust-overlay)
          (self: super: {
            rustToolchain = super.rust-bin.stable.latest.default.override {
              extensions = ["rustfmt" "clippy" "rust-src"];
            };
          })
        ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in {
        devShells.default = with pkgs;
          mkShell {
            nativeBuildInputs =
              (with pkgs; [
                rustToolchain
                openssl
                postgresql
                pkg-config
              ])
              ++ pkgs.lib.optionals pkgs.stdenv.isDarwin (with pkgs; [
                ]);

            RUSTFMT = "${pkgs.rust-bin.nightly.latest.rustfmt}/bin/rustfmt";
          };
      }
    );
}
