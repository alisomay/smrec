{
  # Define the inputs required for this flake.
  inputs = {
    # The version of nixpkgs we're using. We need a newer rustc that is available on the unstable banch.
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable"; 

    # The flake-utils provide helpful utilities for managing flakes.
    utils.url = "github:numtide/flake-utils";

    # naersk is used for building Rust projects with Nix.
    naersk.url = "github:nix-community/naersk/master";
  };

  # Define the outputs of the flake, which depend on the inputs and the current flake.
  outputs = { self, nixpkgs, utils, naersk }:
    # Use flake-utils to generate outputs for each system supported by default.
    utils.lib.eachDefaultSystem (system:
      let
        # Import the nixpkgs for the given system.
        pkgs = import nixpkgs { inherit system; };

        # Instantiate naersk with the current package set.
        naerskLib = pkgs.callPackage naersk { };

        # Name of the Rust project.
        name = "smrec";

        # Build-time dependencies.
        nativeBuildInputs = with pkgs; [
          cargo      # Cargo, the Rust package manager.
          rustc      # The Rust compiler.
          pkg-config # Helper tool used when compiling applications and libraries.
        ];

        # Runtime dependencies.
        buildInputs = with pkgs; [
          alsa-lib # ALSA library for audio.
          jack2    # JACK Audio Connection Kit.
        ];
      in
      {
        # Define a development shell for this project.
        devShells.default = pkgs.mkShell
          {
            # Pass both build-time and runtime dependencies to the shell environment.
            inherit buildInputs nativeBuildInputs;
          };

        # Define the Rust package.
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = name;             # Package name.
          version = "0.2.1";        # Version of the package.
          inherit buildInputs nativeBuildInputs;
          src = ./.;                # Source directory for the Rust project.
          cargoLock = {
            lockFile = ./Cargo.lock; # Path to the Cargo.lock file.
            # Specific output hashes for dependencies, required for reproducibility.
            outputHashes = {
              "asio-sys-0.2.1" = "sha256-MPknKFVyxTDI7r4xC860RSOa9zmB/iQsCZeAlvE8cdk=";
            };
          };
        };

        # Define the application produced by this project.
        apps.default = {
          type = "app"; # Type is an application.
          # The path to the executable that will run the app.
          program = "${self.packages.${system}.default}/bin/${name}";
        };
      });
}
