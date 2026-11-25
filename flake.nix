{
  description = "ranger development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, fenix, ... }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system} system);
    in
    {
      devShells = forAllSystems (
        pkgs: system: {
          default = pkgs.mkShell {
            packages = [
              # grcov needs llvm-tools from the same toolchain as rustc, which nixpkgs' rustc doesn't ship.
              (fenix.packages.${system}.stable.withComponents [
                "cargo"
                "clippy"
                "llvm-tools"
                "rust-src"
                "rustc"
                "rustfmt"
              ])
              pkgs.bc
              pkgs.dotslash
              pkgs.entr
              pkgs.fd
              pkgs.jq
              pkgs.sqlite
            ];

            # Match CI: grcov, just, and zizmor come from the DotSlash files in bin/.
            shellHook = ''
              export PATH="$PWD/bin:$PATH"
            '';
          };
        }
      );
    };
}
