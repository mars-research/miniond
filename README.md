# miniond

[![Build](https://github.com/mars-research/miniond/actions/workflows/build.yml/badge.svg)](https://github.com/mars-research/miniond/actions/workflows/build.yml)

```
[2021-08-29T22:58:26Z INFO ] miniond 0.1.0 starting
[2021-08-29T22:58:26Z INFO ] Looking for the boss node...
[2021-08-29T22:58:26Z INFO ] Discovered boss node from /etc/resolv.conf: 128.104.222.9
[2021-08-29T22:58:26Z INFO ] Starting all applets...
[2021-08-29T22:58:26Z INFO ] Informing testbed that we have booted...
[2021-08-29T22:58:26Z INFO ] Reloading information from testbed...
[2021-08-29T22:58:29Z INFO ] Got new mount configurations (2 mounts)
[2021-08-29T22:58:29Z INFO ] Creating systemd unit proj-project\x2dPG0.mount for ops.wisc.cloudlab.us:/proj/project-PG0...
[2021-08-29T22:58:29Z INFO ] Got new account configurations (Users: 19, Groups: 1)
[2021-08-29T22:58:29Z INFO ] Updating user zhaofeng with UID 20001...
[2021-08-29T22:58:29Z INFO ] Updating SSH keys for user zhaofeng...
[...]
[2021-08-29T22:58:29Z INFO ] Successfully applied account configurations
[2021-08-29T22:58:29Z INFO ] Informing testbed that we are ready...
[2021-08-29T22:58:30Z INFO ] Creating systemd unit share.mount for ops.wisc.cloudlab.us:/share...
```

`miniond` is an alternative implementation of the Emulab client-side agents in Rust.
It enables the use of arbitrary Linux distributions on CloudLab/Emulab, while still offering the convenience users normally expect from a regular Emulab image (testbed user creation, auto NFS mounts, etc.).
It's designed to be simple and can be compiled into a single statically-linked binary with [musl libc](https://www.musl-libc.org).

The official implementation of [Emulab Clientside](https://wiki.emulab.net/wiki/Emulab/wiki/ClientSideStuff) is complex, consisting of multiple binaries and scripts with intricate dependencies.
It makes a lot of assumptions about the filesystem layout and utilizes numerous non-standard directories (see the *Notes on Emulab Clientside* section), making installation on a clean OS extremely difficult.
The goal of `miniond` is to provide a simple implementation of Emulab Clientside that is easy to install on top of a clean OS.

## Features

- [x] Report to the testbed that the node is ready
- [x] Set the system hostname
- [x] Create testbed users and add SSH keys
- [x] Mount NFS filesystems
- [ ] Set up IP addresses on experimental interfaces
- [ ] Report load average and other statistics to the testbed

## Usage 

### NixOS

It's recomended to use the provided NixOS modules to set up the entire system for Emulab:

```nix
{
  imports = [
    /path/to/miniond/nixos/recommended-no-flakes.nix
  ];
}
```

To install NixOS, use another image (e.g., UBUNTU20-64-STD) and follow [the *Installing from another Linux distribution* section](https://nixos.org/manual/nixos/stable/#sec-installing-from-other-distro) in the NixOS manual.

It's also possible to customize the `miniond` configuration.
See `nixos/miniond.nix` for details.

### Manual / Other Distributions

The following commands must be available in the PATH:
- `useradd`
- `usermod`
- `groupadd`
- `groupmod`
- `systemctl` (if using systemd for mounting)
- ~~`mount` (if not using systemd for mounting)~~ (not implemented)

The `bash` and `tcsh` shells should be installed and configured in `/etc/shells`.
The "admin group" (normally `wheel` or `sudo`) should be configured to allow passwordless privilege escalation.

To deploy `miniond`, create a file named `miniond.toml`:

```toml
# Auto account management
[autouser]
enable = true          # default: true
# admin-group = "root" # default: automatically discover and fall back to "root"

# Auto NFS Mount
[automount]
enable = true          # default: true
# backend = "systemd"  # default: "systemd"

# Auto hostname
[autohost]
enable = true          # default: true

# Systemd integration
[systemd]
# unit-dir = "/etc/systemd/system"

# TMCC
[tmcc]
# You can manually specify the boss node, if desired.
# By default, miniond automatically discovers the boss node using the same
# steps as the official implementation:
#
#     https://wiki.emulab.net/wiki/TmcdApi
#
# boss = boss.wisc.cloudlab.us
# port = 7777
```

Run `miniond` on boot, preferably as a system service:

```
miniond -f /path/to/miniond.toml
```

If you are using systemd, a sample service configuration is provided at `example/miniond.service`.

## Development

`miniond` is a normal Cargo project and can be built with `cargo build`.

As a single-binary daemon, `miniond` implements distinct features as "applets."
Applets run concurrently and communicate with each other via a Tokio broadcast channel (think of it as a shared bus).

It's strongly recommended to use [Nix](https://github.com/numtide/nix-unstable-installer) to manage development dependencies.
With Nix installed, use `nix-shell` or `nix develop` to enter the development environment.

## Notes on Emulab Clientside

To quote the official [Emulab Clientside install guide](https://wiki.emulab.net/wiki/ClientSideInstall):

> Note that we strongly urge you NOT to build your own image from scratch, but start with an existing image from Utah. This process is just too much of **a pain in the ass** and too complicated to describe completely, and you will pull less of your hair out by starting with an existing image.

(Emphasis ours - We agree with the description)

In order to aid the development of `miniond`, we have put together a small set of notes on the official implementation.

### File System

In the `UBUNTU20-64-STD` image, the official Emulab Clientside is installed to the following paths:

```
/etc/emulab                               (configurations)
/etc/testbed                              (link to /etc/emulab)
/etc/systemd/system                       (various services)
/usr/local/etc/emulab                     (binaries and scripts - not configurations!)
/usr/local/lib/emulabclient.py
/usr/local/libexec                        (binaries)
/usr/local/{bin,sbin}                     (binaries)
/usr/testbed                              (links to binaries under /usr/local/etc/emulab)
``` 

### Boot Process

CloudLab/Emulab machines always boot via PXE which starts a minimal FreeBSD-based system that acts like a bootloader.
This minimal system is responsible for downloading and applying the specified image for the experiment.
After loading the desired image, the bootloader then loads the MBR header *from the first partition* (not from the disk).

In NixOS, to force GRUB to install to a partition:

```
{
  boot.loader.grub = {
    device = "/dev/sda1";
    forceInstall = true;
  }
}
```

## Limitations & Behavioral Differences

`miniond` is an early prototype.
It behaves differently from the official Emulab Clientside in the following manners:

- [File system cleanup](https://gitlab.flux.utah.edu/emulab/emulab-devel/-/blob/master/clientside/tmcc/linux/prepare) prior to imaging has not yet been implemented. If you create a disk image, sensitive files may be left over.
- `miniond` creates project groups with lower-case names (`project-pg0` instead of `project-PG0`). The reason is that group names with upper-case letters are unsupported by upstream `shadow-utils`.

## Licensing

`miniond` is available under the **GNU Affero General Public License, version 3**, matching [the official implementation](https://gitlab.flux.utah.edu/emulab/emulab-devel).
See `LICENSE` for details.
