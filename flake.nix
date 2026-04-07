{
  description = "A very basic flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    {
      packages.x86_64-linux = {
        unwrapped = throw "The package for buttplug-lite has been removed, use the 'buttplug-lite' package from nixpkgs instead.";
        default = throw "The package for buttplug-lite has been removed, use the 'buttplug-lite' package from nixpkgs instead.";
      };
    };
}
