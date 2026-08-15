{
  description = "Port of Composer, the dependency manager for PHP, written in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    systems.url = "github:nix-systems/x86_64-linux";

    flake-utils = {
      url = "github:numtide/flake-utils";
      inputs.systems.follows = "systems";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      rust-overlay,
      treefmt-nix,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        treefmt = treefmt-nix.lib.evalModule pkgs {
          imports = [ ./treefmt.nix ];
          programs.rustfmt.package = rustToolchain;
        };
      in
      {
        formatter = treefmt.config.build.wrapper;

        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            pkgs.php85

            # The following softwares are used for testing.
            # VCSs
            pkgs.fossil
            pkgs.git
            pkgs.mercurial
            pkgs.subversion
            # Archivers
            pkgs.bzip2
            pkgs.gnutar
            pkgs.gzip
            pkgs.p7zip
            pkgs.unzip
            pkgs.xz
            pkgs.zip
          ];
        };
      }
    );
}
