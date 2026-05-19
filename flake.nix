{
  description = "Mue Development Environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, utils }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        build-cli = pkgs.writeShellApplication {
          name = "build";
          runtimeInputs = [ pkgs.cargo pkgs.rustc pkgs.git pkgs.clippy ];
          text = ''
            ROOT=$(git rev-parse --show-toplevel)
            cd "$ROOT/rust"
            
            cargo clippy --workspace -- -D warnings

            if [ "''${1:-}" = "--release" ]; then
              cargo build --release --workspace
            else
              cargo build --workspace "$@"
            fi
          '';
        };

        mue-bin = pkgs.writeShellApplication {
          name = "mue";
          runtimeInputs = [ pkgs.git ];
          text = ''
            ROOT=$(git rev-parse --show-toplevel)
            BIN="$ROOT/rust/target/release/mue"
            
            if [ ! -f "$BIN" ]; then
              echo "Error: mue binary not found at $BIN."
              echo "Run 'build --release' first."
              exit 1
            fi
            
            exec "$BIN" "$@"
          '';
        };

        muex-bin = pkgs.writeShellApplication {
          name = "muex";
          runtimeInputs = [ pkgs.git ];
          text = ''
            ROOT=$(git rev-parse --show-toplevel)
            BIN="$ROOT/rust/target/release/muex"
            
            if [ ! -f "$BIN" ]; then
              echo "Error: muex binary not found at $BIN."
              echo "Run 'build --release' first."
              exit 1
            fi
            
            exec "$BIN" "$@"
          '';
        };

        devPackages = with pkgs; [
          pkg-config
          openssl
          cargo
          rustc
          rust-analyzer
          rustfmt
          clippy
          build-cli
          mue-bin
          muex-bin
          nix
          intermodal
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = devPackages;
          shellHook = ''
            export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig"
          '';
        };
      }
    );
}
