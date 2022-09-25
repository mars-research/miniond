{
  description = "Alternative implementation of the Emulab client-side programs";

  inputs = {
    mars-std.url = "github:mars-research/mars-std";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, mars-std, crane, ... }: let
    # System types to support.
    supportedSystems = [ "x86_64-linux" ];

    # Rust nightly version.
    nightlyVersion = "2022-09-15";
  in mars-std.lib.eachSystem supportedSystems (system: let
    pkgs = mars-std.legacyPackages.${system};

    inherit (pkgs) lib;
    inherit (pkgs.rust) toRustTargetSpec;

    targetOverrides = {
      "x86_64-linux" = "x86_64-unknown-linux-musl";
    };

    buildTarget = targetOverrides.${system} or (toRustTargetSpec pkgs.stdenv.hostPlatform);

    rustNightly = pkgs.rust-bin.nightly.${nightlyVersion}.default.override {
      extensions = [ "rust-src" "rust-analyzer-preview" ];
      targets = [
        (toRustTargetSpec pkgs.stdenv.hostPlatform)
        buildTarget
      ];
    };

    craneLib = (crane.mkLib pkgs).overrideToolchain rustNightly;

    miniond = craneLib.buildPackage {
      src = craneLib.cleanCargoSource ./.;
      cargoExtraArgs = "--target ${buildTarget}";
    };
  in rec {
    packages = {
      inherit miniond;
    };
    defaultPackage = self.packages.${system}.miniond;

    devShell = pkgs.mkShell {
      inputsFrom = [ defaultPackage ];
      nativeBuildInputs = with pkgs; [
        cargo-outdated
      ];
    };
  }) // {
    nixosModule = { pkgs, ... }: {
      imports = [
        ./nixos/emulab.nix
        ./nixos/miniond.nix
      ];

      nixpkgs.overlays = [
        (_: super: {
          miniond = self.packages.${pkgs.system}.miniond;
        })
      ];
    };

    nixosConfigurations.testSystem = mars-std.inputs.nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        self.nixosModule
        ({ pkgs, lib, ... }: {
          hardware.emulab.enable = true;
          boot.isContainer = true;
        })
      ];
    };
  };
}
