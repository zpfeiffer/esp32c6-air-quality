{
  description = "Air Dev Shell Flake";

  inputs = {
    flake-parts.url = "github:hercules-ci/flake-parts";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    git-hooks-nix.url = "github:cachix/git-hooks.nix";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    esp-dev = {
      url = "github:mirrexagon/nixpkgs-esp-dev";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {flake-parts, ...}:
    flake-parts.lib.mkFlake {inherit inputs;} ({...}: {
      debug = true;

      imports = [
        inputs.git-hooks-nix.flakeModule
      ];

      systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin"];
      perSystem = {
        config,
        pkgs,
        system,
        ...
      }: let
        rust-toolchain =
          pkgs.rust-bin.fromRustupToolchainFile
          ./rust-toolchain.toml;
      in {
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [
            inputs.esp-dev.overlays.default
            inputs.rust-overlay.overlays.default
          ];
          config = {};
        };

        # Define the formatter used by `nix fmt`
        formatter = pkgs.alejandra;

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rust-toolchain

            # CLI tool for on-chip debugging and flashing
            probe-rs-tools

            # https://docs.esp-rs.org/book/writing-your-own-application/generate-project/esp-generate.html
            esp-generate

            # Provided by esp-dev overlay
            esp-idf-esp32c6
          ];

          packages = [];
          inputsFrom = [];
          shellHook = ''
            # Install pre-commit hooks
            ${config.pre-commit.installationScript}
            echo 1>&2 "Started Air development shell"
          '';
        };

        pre-commit = {
          # Run pre-commit hooks as part of `nix flake check`
          check.enable = true;
          settings.hooks = {
            # Nix formatter
            alejandra.enable = true;

            # Nix linter
            statix.enable = true;

            mixed-line-endings.enable = true;
            end-of-file-fixer.enable = true;
          };
        };
      };
    });
}
