{ pkgs, ... }:
let
  miniond = import ../default.nix;
in {
  imports = [
    ./emulab.nix
    ./miniond.nix
  ];

  nixpkgs.overlays = [
    (self: super: {
      inherit miniond;
    })
  ];

  hardware.emulab.enable = true;
}
