{
  description = "Unofficial, browser-free CLI for Kleinanzeigen";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "kleinanzeigen-cli";
            version = "0.1.0";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
            meta = {
              description = "Unofficial, browser-free CLI for Kleinanzeigen";
              homepage = "https://github.com/fleischerdesign/kleinanzeigen-cli";
              license = nixpkgs.lib.licenses.mit;
              mainProgram = "kleinanzeigen";
            };
          };
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              clippy
              rust-analyzer
              rustc
              rustfmt
            ];
            shellHook = ''
              echo "kleinanzeigen-cli: cargo test, cargo clippy, nix flake check"
            '';
          };
        }
      );

      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt-tree);

      checks = forAllSystems (system: {
        default = self.packages.${system}.default;
      });
    };
}
