# Miniond NixOS module

{ pkgs, lib, config, ... }:
with lib;
let
  cfg = config.services.miniond;

  configType = types.submodule {
    options = {
      autouser = {
        enable = mkOption {
          description = "Automatically configure users and SSH keys.";
          type = types.bool;
          default = true;
        };
        admin-group = mkOption {
          description = "Group of the admin user.";
          type = types.str;
          default = "wheel";
        };
      };
      automount = {
        enable = mkOption {
          description = "Automatically mount configured NFS shares.";
          type = types.bool;
          default = true;
        };
        backend = mkOption {
          description = "Method to mount NFS shares.";
          type = types.enum [ "systemd" ];
          default = "systemd";
        };
      };
      autohost = {
        enable = mkOption {
          description = "Automatically set system hostname.";
          type = types.bool;
          default = true;
        };
        etc_hosts = mkOption {
          description = "Path to the /etc/hosts file.";
          type = types.nullOr types.path;
          default = null;
        };
      };
      tmcc = {
        boss = mkOption {
          description = ''
            The boss node.

            By default this will be automatically discovered.
          '';
          type = types.nullOr types.str;
          default = null;
        };
        port = mkOption {
          description = "The TMCD port.";
          type = types.nullOr types.ints.unsigned;
          default = null;
        };
        report-shutdown = mkOption {
          description = "Whether to report shutdowns to the testbed.";
          type = types.bool;
          default = true;
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

  # https://stackoverflow.com/a/58022572
  configFile = pkgs.runCommand "miniond.conf" {
    config = builtins.toJSON cfg.configuration;
  } ''
    echo "$config" \
    | ${pkgs.jq}/bin/jq 'walk( if type == "object" then with_entries(select(.value != null)) else . end)' \
    | ${pkgs.remarshal}/bin/remarshal --if json --of toml > $out
  '';
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
    environment.etc."miniond.conf".source = configFile;

    systemd.services.miniond = {
      wants = [ "network-online.target" ];
      after = [ "network-online.target" ];
      wantedBy = [ "multi-user.target" ];

      path = with pkgs; [ shadow systemd ];

      description = "Emulab Testbed Agent";
      serviceConfig = {
        TimeoutStopSec = 10;

        ExecStart = "${pkgs.miniond}/bin/miniond -f /etc/miniond.conf";
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
