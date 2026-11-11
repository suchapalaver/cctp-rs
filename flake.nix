# SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
# SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
#
# SPDX-License-Identifier: Apache-2.0

{
  description = "Rust SDK for Circle's Cross-Chain Transfer Protocol (CCTP)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    let
      # nixos-unstable dropped x86_64-darwin in 26.11. Keep the declared
      # outputs aligned with platforms that the selected Nixpkgs can evaluate.
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
    in
    flake-utils.lib.eachSystem supportedSystems (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        cargoManifest = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        nativeBuildInputs = with pkgs; [
          rustToolchain
          pkg-config
        ];

        # Darwin stdenv supplies the default Apple SDK, including the Security
        # and SystemConfiguration frameworks used by the Rust dependency graph.
        buildInputs = with pkgs; [
          openssl
        ];
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "cctp-rs";
          version = cargoManifest.package.version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          inherit nativeBuildInputs buildInputs;

          # Skip tests that require network access
          doCheck = false;
        };

        devShells.default = pkgs.mkShell {
          inherit buildInputs;

          nativeBuildInputs = nativeBuildInputs ++ (with pkgs; [
            # Rust tools
            cargo-watch
            cargo-edit

            # For SPDX license compliance
            reuse

            # Lean 4 toolchain manager for verification/ (version pinned
            # by verification/lean-toolchain)
            elan

            # Other useful tools
            git
          ]);

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };
      }
    );
}
