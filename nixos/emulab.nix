# Emulab NixOS module

{ pkgs, lib, config, ... }:
with lib;
let
  cfg = config.hardware.emulab;
in {
  options = {
    hardware.emulab = {
      enable = mkEnableOption "Enable various configurations for Emulab testbeds";
    };
  };

  config = lib.mkIf cfg.enable {
    services.miniond.enable = true;

    boot.loader.grub.device = mkDefault "/dev/sda1";
    boot.loader.grub.forceInstall = mkDefault true;

    boot.loader.grub.extraConfig = ''
      serial --unit=0 --speed=115200 --word=8 --parity=no --stop=1
      terminal_input --append serial
      terminal_output --append serial
    '';
  
    boot.kernelParams = [ "console=ttyS0,115200n8" ];

    # Serial access is authenticated
    services.getty.autologinUser = mkDefault "root";

    # Passwordless sudo access
    security.sudo.wheelNeedsPassword = mkDefault false;
  };
}
