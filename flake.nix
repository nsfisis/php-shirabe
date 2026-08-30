{
  description = "Port of Composer, the dependency manager for PHP, written in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    systems.url = "github:nix-systems/default";

    composer = {
      url = "github:composer/composer/2.9.7";
      flake = false;
    };

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
      self,
      nixpkgs,
      composer,
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

        php = pkgs.php85;

        composerVersion =
          let
            versionLines = builtins.filter (match: match != null) (
              map (line: builtins.match " *public const VERSION = '(.*)';" line) (
                pkgs.lib.splitString "\n" (builtins.readFile "${composer}/src/Composer/Composer.php")
              )
            );
          in
          builtins.head (builtins.head versionLines);

        composerRuntime = pkgs.stdenvNoCC.mkDerivation {
          pname = "composer-runtime";
          version = composerVersion;
          src = composer;
          nativeBuildInputs = [
            php
            php.packages.composer
          ];
          dontPatchShebangs = true;
          buildPhase = ''
            runHook preBuild
            export COMPOSER_HOME="$TMPDIR/composer-home"
            export COMPOSER_CACHE_DIR="$TMPDIR/composer-cache"
            export COMPOSER_ROOT_VERSION="${composerVersion}"
            composer install --no-dev --no-interaction --no-progress
            runHook postBuild
          '';
          installPhase = ''
            runHook preInstall
            cp -r . "$out"
            runHook postInstall
          '';

          outputHashMode = "recursive";
          outputHashAlgo = "sha256";
          outputHash = "sha256-owdTo9YBaQv3a3FnUubBfI0gjca5XEJ84/PukcukPfc=";
        };

        commitTime = self.lastModified;
        commitDate = self.lastModifiedDate;

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        shirabe = rustPlatform.buildRustPackage {
          pname = "shirabe";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;

          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./crates
            ];
          };
          cargoLock.lockFile = ./Cargo.lock;

          postPatch = ''
            cp -r ${composerRuntime} composer
            chmod -R u+w composer
          '';

          env = {
            SHIRABE_RELEASE_DATE =
              let
                field = start: builtins.substring start 2 commitDate;
              in
              "${builtins.substring 0 4 commitDate}-${field 4}-${field 6} ${field 8}:${field 10}:${field 12}";
            SHIRABE_DEV_WARNING_TIME = toString (commitTime + 60 * 86400);
          };

          doCheck = false;

          meta = {
            description = "Rust port of Composer, the dependency manager for PHP";
            homepage = "https://github.com/nsfisis/php-shirabe";
            license = pkgs.lib.licenses.mit;
            mainProgram = "shirabe";
          };
        };
      in
      {
        formatter = treefmt.config.build.wrapper;

        packages = {
          inherit shirabe;
          default = shirabe;
        };

        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            php
            php.packages.composer
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
