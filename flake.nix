{
  description = "Port of Composer, the dependency manager for PHP, written in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    systems.url = "github:nix-systems/default";

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
            pkgs.php85Packages.composer
            pkgs.just

            # The following softwares are used for testing:

            # VCSs
            pkgs.git
            # VCSs (optional)
            #   Neither Composer's test suite nor Shirabe's uses a real binary
            #   for these commands: all tests for them mock a process executor.
            #   They only slow the tests down.
            #   Please uncomment to run those code paths by hand.
            # pkgs.fossil
            # pkgs.mercurial
            # pkgs.subversion

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
