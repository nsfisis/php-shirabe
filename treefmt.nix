{ pkgs, ... }:
{
  projectRootFile = "flake.nix";

  programs.nixfmt.enable = true;
  programs.rustfmt.enable = true;

  settings.formatter.mago = {
    command = pkgs.lib.getExe pkgs.mago;
    options = [ "format" ];
    includes = [
      "*.php"
      "scripts/linters/lint"
      "scripts/linters/upstream-file-hash"
      "scripts/plugin-class-classifier/classify"
      "scripts/plugin-class-classifier/query"
      "scripts/plugin-stub-generator/generate-stubs"
    ];
  };
}
