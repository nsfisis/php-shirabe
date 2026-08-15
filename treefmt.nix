{ ... }:
{
  projectRootFile = "flake.nix";

  programs.nixfmt.enable = true;
  programs.rustfmt.enable = true;
  # TODO: add php formatter
}
