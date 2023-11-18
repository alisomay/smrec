{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable"; # because we need rustc 1.70
    utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk/master";
  };
  outputs = { self, nixpkgs, utils, naersk }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        naerskLib = pkgs.callPackage naersk { };
        name = "smrec";
        nativeBuildInputs = with pkgs; [
          cargo
          rustc
          pkg-config
        ];
        buildInputs = with pkgs; [
          alsa-lib
          jack2
        ];
      in
      {
        devShells.default = pkgs.mkShell
          {
            inherit buildInputs nativeBuildInputs;
          };
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = name;
          version = "0.2.0";
          inherit buildInputs nativeBuildInputs;
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "asio-sys-0.2.1" = "sha256-MPknKFVyxTDI7r4xC860RSOa9zmB/iQsCZeAlvE8cdk=";
            };
          };
        };
        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/${name}";
        };
      });
}
