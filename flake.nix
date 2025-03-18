{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      utils,
      naersk,
    }:
    utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        naersk-lib = pkgs.callPackage naersk { };
      in
      {
        defaultPackage = naersk-lib.buildPackage ./.;
        devShell =
          with pkgs;
          mkShell.override { stdenv = pkgs.clangStdenv; } rec {
            nativeBuildInputs = [
              cargo
              rustc
              rustfmt
              rustPackages.clippy
              mold
              pre-commit
            ];
            buildInputs = [
              speechd.out
            ];
            shellHook = ''
              export RUSTFLAGS="-C link-arg=-Wl,-rpath,${lib.makeLibraryPath buildInputs}";
              export LIBCLANG_PATH="${pkgs.libclang.lib}/lib";
              pre-commit install
            '';
            RUST_SRC_PATH = rustPlatform.rustLibSrc;
          };
      }
    );
}
