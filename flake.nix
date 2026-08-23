{
  description = "Gardn — terminal workspace manager for AI coding agents";

  inputs = {
    # Nixpkgs 26.11 dropped x86_64-darwin; 26.05 supports every release target.
    # Reconsider this pin when 26.05 reaches end of support or we drop Intel macOS.
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
          gardn = pkgs.callPackage ./nix/package.nix { };
        in
        {
          inherit gardn;
          default = gardn;
        }
      );

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/gardn";
          meta.description = "Run Gardn";
        };
      });

      checks = forAllSystems (system: {
        gardn = self.packages.${system}.default;
        default = self.checks.${system}.gardn;
      });

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          default = pkgs.mkShell {
            name = "gardn-dev";
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
              zig_0_16
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
        gardn = final.callPackage ./nix/package.nix { };
      };
    };
}
