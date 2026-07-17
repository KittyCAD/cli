{
  description = "cli development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
  }: let
    overlays = [
      (import rust-overlay)
      (self: super: {
        rustToolchain = super.rust-bin.stable.latest.default.override {
          targets = ["wasm32-unknown-unknown"];
          extensions = ["rustfmt" "llvm-tools-preview" "rust-src"];
        };

        # stand-alone nightly formatter so we get the fancy unstable flags
        nightlyRustfmt = super.rust-bin.selectLatestNightlyWith (toolchain:
          toolchain.default.override {
            extensions = ["rustfmt"]; # just the formatter
          });
      })
    ];

    allSystems = [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ];

    forAllSystems = f:
      nixpkgs.lib.genAttrs allSystems (system:
        f {
          pkgs = import nixpkgs {
            inherit overlays system;
          };
          system = system;
        });
  in {
    devShells = forAllSystems ({pkgs, ...}: {
      default = pkgs.mkShell {
        packages =
          (with pkgs; [
            rustToolchain
            nightlyRustfmt
            cargo-sort
            toml-cli
            openssl
            postgresql
            pkg-config
          ])
          ++ pkgs.lib.optionals pkgs.stdenv.isDarwin (with pkgs; [
            ]);

        RUSTFMT = "${pkgs.nightlyRustfmt}/bin/rustfmt";
      };
    });

    packages = forAllSystems ({
      pkgs,
      system,
    }: let
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      rustPlatform = pkgs.makeRustPlatform {
        cargo = pkgs.rustToolchain;
        rustc = pkgs.rustToolchain;
      };
    in {
      zoo = rustPlatform.buildRustPackage {
        pname = "zoo";
        version = cargoToml.package.version;
        src = ./.;

        cargoLock = {
          lockFile = ./Cargo.lock;
          outputHashes = {
            "openapitor-0.0.9" = "sha256-UpyQzk4VnqNKwS2DUz9tM+v5YKEVoNkd9GyzaGX1uzk=";
          };
        };

        doCheck = false;
        nativeBuildInputs = [pkgs.pkg-config];
        buildInputs = [pkgs.openssl];
      };
      default = self.packages.${system}.zoo;
    });
  };
}
