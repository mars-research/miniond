# Prebuilt NixOS Image

A prebuilt image of a minimal NixOS 21.11 installation with miniond is available at:

```
urn:publicid:IDN+wisc.cloudlab.us+image+redshift-PG0:nixos-2111
```

## Configuration

The system configuration source can be edited in `/etc/nixos`.
After editing the configuration, rebuild the system with `sudo nixos-rebuild switch`.
See [the NixOS manual](https://nixos.org/manual/nixos/stable) for more details.

It's highly recommended to [pin the version of Nixpkgs](https://nix.dev/reference/pinning-nixpkgs) for reproducibility.

## Disk Image Creation

When disk imaging is initiated through the web portal, the following things will happen:

1. Nix Store garbage collection is performed (`nix-collect-garbage -d`).
1. The system is rebooted into Stage 1 initrd via kexec.
1. All files in the file system are deleted, except for `/nix`, `/boot`, and `/etc/nixos`.
    - `/etc/nixos` is preserved to facilitate rebuilding the system for users of the image. If you manage the system configurations by other means, please add appropriate instructions to the image description so others can reproduce your setup.
    - You can preserve additional paths by adding them to `hardware.emulab.allowedImpurities`.
1. The system is rebooted to the Emulab Admin MFS for imaging.

## Resources

- [Nix Tutorials](https://nix.dev/tutorials)
- [NixOS Manual](https://nixos.org/manual/nixos/stable)
- [Nix Package Search](https://search.nixos.org)
- [NixOS Option Search](https://search.nixos.org/options)
