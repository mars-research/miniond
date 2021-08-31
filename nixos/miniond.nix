# Miniond NixOS module

{ pkgs, lib, config, ... }:
with lib;
let
  configType = types.submodule {
    options = {
      autouser = {
        enable = mkOption {
          description = "Automatically configure users and SSH keys";
          type = types.bool;
          default = true;
        };
        "admin-group" = mkOption {
          description = "Group of the admin user";
          type = types.str;
          default = "wheel";
        };
      };
      automount = {
        enable = mkOption {
          description = "Automatically mount configured NFS shares";
          type = types.bool;
          default = true;
        };
        backend = mkOption {
          description = "Method to mount NFS shares";
          type = types.enum [ "systemd" ];
          default = "systemd";
        };
      };
      systemd = {
        "unit-dir" = mkOption {
          description = "Path to write systemd unit files";
          type = types.path;
          default = "/run/systemd-miniond/system";
        };
      };
    };
  };

  configFile = let
    c = cfg.configuration;
    renderBool = b: if b then "true" else "false";
  in pkgs.writeText "miniond.toml" ''
    [autouser]
    enable = ${renderBool c.autouser.enable}
    admin-group = "${c.autouser.admin-group}"

    [automount]
    enable = ${renderBool c.automount.enable}
    backend = "${c.automount.backend}"

    [systemd]
    unit-dir = "${c.systemd.unit-dir}"
  '';

  cfg = config.services.miniond;
in {
  options = {
    services.miniond = {
      enable = mkEnableOption "Enable miniond, alternative implementation of Emulab Clientside";
      configuration = mkOption {
        description = ''
          miniond configurations.

          See https://github.com/mars-research/miniond for more information.
        '';
        type = configType;
        default = {};
      };
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.services.miniond = {
      wants = [ "network-online.target" ];
      after = [ "network-online.target" ];
      wantedBy = [ "multi-user.target" ];

      path = with pkgs; [ shadow systemd ];

      description = "Emulab Testbed Agent";
      serviceConfig = {
        TimeoutStopSec = 10;

        ExecStart = "${pkgs.miniond}/bin/miniond -f ${configFile}";
        ExecReload = "${pkgs.coreutils}/bin/kill -HUP $MAINPID";
      };
    };

    boot.extraSystemdUnitPaths = [ cfg.configuration.systemd.unit-dir ];
    boot.supportedFilesystems = [ "nfs" ];

    # A hack to allow mutation of /etc/hosts
    system.activationScripts.miniond-mutable-hosts = stringAfter [ "etc" ] ''
      echo "setting up mutable /etc/hosts..."
      cat /etc/hosts > /etc/hosts.real
      rm /etc/hosts
      cp /etc/hosts.real /etc/hosts

      if /run/current-system/systemd/bin/systemctl --quiet is-active miniond; then
        echo "reloading miniond..."
        /run/current-system/systemd/bin/systemctl reload miniond
      fi
    '';
  };
}
