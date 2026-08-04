{
  description = "Oh My Herdr — terminal workspace manager for AI coding agents";

  inputs = {
    # Nixpkgs 26.11 dropped x86_64-darwin; 26.05 supports every release target.
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-26.05-darwin";
  };

  outputs =
    { self, nixpkgs }:
    let
      lib = nixpkgs.lib;
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = lib.genAttrs systems;
      pkgsFor = system: import nixpkgs { inherit system; };
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          omh = pkgs.callPackage ./nix/package.nix { };
        in
        {
          inherit omh;
          default = omh;
        }
      );

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/omh";
          meta.description = "Run Oh My Herdr";
        };
      });

      checks = forAllSystems (system: {
        omh = self.packages.${system}.default;
        default = self.checks.${system}.omh;
      });

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          default = pkgs.mkShell {
            name = "omh-dev";
            packages = with pkgs; [
              cargo
              cargo-nextest
              clippy
              cmake
              just
              ninja
              pkg-config
              rustc
              rustfmt
              zig_0_15
            ];

            env = {
              LIBGHOSTTY_VT_OPTIMIZE = "Debug";
              LIBGHOSTTY_VT_SIMD = "true";
            };
          };
        }
      );

      formatter = forAllSystems (system: (pkgsFor system).nixfmt);

      overlays.default = final: _prev: {
        omh = final.callPackage ./nix/package.nix { };
      };
    };
}
