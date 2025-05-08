{
  description = "A very basic flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      pkgs = import nixpkgs { system = "x86_64-linux"; };
      libs =
        with pkgs;
        with pkgs.xorg;
        [
          libX11
          libGL
          libxcb
          libxkbcommon
          dbus.dev
          udev.dev
          openssl.dev
          wayland
        ];
      libraryPath = "${pkgs.lib.makeLibraryPath libs}";
    in
    {
      packages.x86_64-linux = rec {
        unwrapped = pkgs.rustPlatform.buildRustPackage {
          pname = "buttplug-lite-unwrapped";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;

          nativeBuildInputs = with pkgs; [
            pkg-config
            git
          ];

          buildInputs = libs;

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          meta = {
            description = "Simplified buttplug.io API for when JSON is infeasible";
            homepage = "https://github.com/runtime-shady-backroom/buttplug-lite";
          };
        };

        default =
          (pkgs.runCommandNoCC "buttplug-lite" {
            pname = "buttplug-lite";
            inherit (unwrapped) version;
            inherit (unwrapped) meta;

            nativeBuildInputs = [ pkgs.makeWrapper ];
          })
            ''
              makeWrapper ${unwrapped}/bin/buttplug-lite $out/bin/buttplug-lite --suffix LD_LIBRARY_PATH : ${libraryPath}
            '';
      };
    };
}
