# Emulab NixOS module

{ pkgs, lib, config, modulesPath, ... }:
with lib;
let
  cfg = config.hardware.emulab;

  allowedImpurities = pkgs.writeText "allowed-impurities" ''
    etc/nixos
    ${builtins.concatStringsSep "\n" cfg.allowedImpurities}
  '';

  prepareScript = pkgs.writeShellScript "emulab-reboot-prepare" ''
    set -euo pipefail

    ${pkgs.util-linux}/bin/wall "Imaging has been initiated on this node. The system is going to be rebooted."
    nix-collect-garbage -d

    cp ${allowedImpurities} /etc/NIXOS_LUSTRATE
    touch /etc/EMULAB_LUSTRATE

    systemctl stop miniond
    systemctl start kexec.target
  '';
in {
  imports = [
    (modulesPath + "/profiles/all-hardware.nix")
  ];

  options = {
    hardware.emulab = {
      enable = mkEnableOption "Enable various configurations for Emulab testbeds";
      enableLustrate = mkEnableOption ''
        Enable pre-imaging filesystem cleanup.

        When imaging is initiated, NIXOS_LUSTRATE will be performed
        and all files not managed by Nix (except for `/etc/nixos` and
        paths listed in `allowedImpurities`) will be deleted.
      '';

      allowedImpurities = mkOption {
        description = ''
          A list of paths to keep during the cleanup process.

          Consider specifying them as part of your system configuration
          instead.
        '';
        type = types.listOf types.path;
        example = [ "/var/lib/experiment/some-stateful-stuff" ];
        default = [];
      };
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

    # Pre-imaging cleanup
    boot.initrd.postMountCommands = optionalString cfg.enableLustrate ''
      if [ -f /mnt-root/old-root/etc/EMULAB_LUSTRATE ]; then
        echo "Actually deleting all impurities"
        rm -rf /mnt-root/old-root

        sync

        echo "Rebooting to Admin MFS..."
        sleep 2
        reboot -f
      fi
    '';

    # HACK: Inject the prepare script at the path Emulab expects.
    # This will not be necessary once <https://groups.google.com/g/cloudlab-users/c/6fRdB7ykOFQ/m/1_HvTebRBgAJ> is implemented
    system.activationScripts.emulab-impure-prepare.text = ''
      echo "setting up Emulab prepare script..."
      mkdir -p /usr/local/etc/emulab
      ln -sf ${prepareScript} /usr/local/etc/emulab/reboot_prepare
    '';
  };
}
