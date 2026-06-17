{
  description = "constatus: Configurable status line for Claude Code";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
    treefmt-nix.url = "github:numtide/treefmt-nix";
    treefmt-nix.inputs.nixpkgs.follows = "nixpkgs";
  };

  # Flake outputs: packages, devShells, and formatters
  outputs = {
    nixpkgs,
    flake-utils,
    rust-overlay,
    crane,
    treefmt-nix,
    ...
  }:
    # Build for all default systems (x86_64-linux, aarch64-linux, etc.)
    flake-utils.lib.eachDefaultSystem (system: let
      # nixpkgs with Rust overlay for latest toolchain
      pkgs = import nixpkgs {
        inherit system;
        overlays = [rust-overlay.overlays.default];
      };
      # Helper to find repo root and run commands from there
      rooted = exec:
        builtins.concatStringsSep "\n"
        [
          ''REPO_ROOT="$(git rev-parse --show-toplevel)"''
          exec
        ];

      # Development shell helper scripts
      scripts = {
        dx = {
          exec = rooted ''$EDITOR "$REPO_ROOT"/flake.nix'';
          description = "Edit flake.nix";
        };
        rx = {
          exec = rooted ''$EDITOR "$REPO_ROOT"/Cargo.toml'';
          description = "Edit Cargo.toml";
        };
      };

      # Convert script definitions into executable derivations
      scriptPackages =
        pkgs.lib.mapAttrs
        (
          name: script:
            pkgs.writeShellApplication {
              inherit name;
              text = script.exec;
              runtimeInputs = script.deps or [];
            }
        )
        scripts;

      # Rust build tools with latest stable toolchain
      craneLib = (crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.stable.latest.default);

      # Build the constatus binary using crane (fast incremental builds)
      constatus = craneLib.buildPackage {
        src = ./.;
        pname = "constatus";
        version = "0.3.0";
        strictDeps = true;
        # ring (via ureq's rustls TLS) needs perl + a C compiler at build time.
        nativeBuildInputs = [pkgs.perl];
      };
    in {
      # Flake outputs for this system
      packages = {
        # Default package: nix build / nix install
        default = constatus;
        # Explicit package: nix build .#constatus
        constatus = constatus;
      };

      # Development shell with all build and formatting tools
      devShells.default = pkgs.mkShell {
        name = "dev";
        buildInputs = with pkgs;
          [
            # Nix tools
            alejandra         # Nix code formatter
            nixd              # Nix language server
            statix            # Nix linter
            deadnix           # Find unused Nix code
            # General tools
            just              # Task runner
            perl              # Build dep for ring (ureq's rustls TLS)
            # Rust toolchain
            rust-bin.stable.latest.default  # Compiler & cargo
            rust-bin.stable.latest.rust-analyzer  # IDE support
          ]
          ++ builtins.attrValues scriptPackages;  # Include helper scripts (dx, rx)
        shellHook = ''
          echo "Welcome to the constatus devshell!"
        '';
      };

      # Code formatters for nix fmt
      formatter = let
        treefmtModule = {
          projectRootFile = "flake.nix";
          programs = {
            alejandra.enable = true;  # Format .nix files
            rustfmt.enable = true;    # Format .rs files
          };
        };
      in
        treefmt-nix.lib.mkWrapper pkgs treefmtModule;
    });
}
