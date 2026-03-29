{
  description = "Halloy - IRC client";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };

          rustToolchain = pkgs.rust-bin.stable.latest.default;

          isDarwin = pkgs.stdenv.isDarwin;

          nativeBuildInputs = [
            rustToolchain
            pkgs.pkg-config
            pkgs.cmake
          ];

          buildInputs =
            [
              pkgs.openssl
              pkgs.xz
            ]
            ++ pkgs.lib.optionals isDarwin [
              pkgs.apple-sdk_15
            ]
            ++ pkgs.lib.optionals (!isDarwin) [
              pkgs.wayland
              pkgs.libxkbcommon
              pkgs.vulkan-loader
              pkgs.xorg.libX11
              pkgs.xorg.libXcursor
              pkgs.xorg.libXrandr
              pkgs.xorg.libXi
            ];

          version = builtins.replaceStrings [ "\n" ] [ "" ] (builtins.readFile ./VERSION);

          halloy = pkgs.rustPlatform.buildRustPackage {
            pname = "halloy";
            inherit version;
            src = pkgs.lib.cleanSource ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };

            inherit nativeBuildInputs buildInputs;

            doCheck = false;

            meta = {
              description = "Halloy IRC Client";
              homepage = "https://halloy.chat";
              license = pkgs.lib.licenses.gpl3Plus;
              mainProgram = "halloy";
            };
          };

          halloyApp = pkgs.stdenv.mkDerivation {
            pname = "Halloy";
            inherit version;
            src = ./assets/macos;

            dontBuild = true;

            installPhase = ''
              mkdir -p "$out/Applications/Halloy.app/Contents/MacOS"
              mkdir -p "$out/Applications/Halloy.app/Contents/Resources"

              cp Halloy.app/Contents/Resources/halloy.icns "$out/Applications/Halloy.app/Contents/Resources/"

              sed -e 's/{{ VERSION }}/${version}/' \
                  -e 's/{{ BUILD }}/1/' \
                  Halloy.app/Contents/Info.plist \
                  > "$out/Applications/Halloy.app/Contents/Info.plist"

              ln -s "${halloy}/bin/halloy" "$out/Applications/Halloy.app/Contents/MacOS/halloy"
            '';

            meta = halloy.meta;
          };
        in
        {
          default = if isDarwin then halloyApp else halloy;
          inherit halloy;
        }
        // pkgs.lib.optionalAttrs isDarwin { app = halloyApp; }
      );

      checks = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };

          rustToolchain = pkgs.rust-bin.stable.latest.default;

          isDarwin = pkgs.stdenv.isDarwin;

          nativeBuildInputs = [
            rustToolchain
            pkgs.pkg-config
            pkgs.cmake
          ];

          buildInputs =
            [
              pkgs.openssl
              pkgs.xz
            ]
            ++ pkgs.lib.optionals isDarwin [
              pkgs.apple-sdk_15
            ]
            ++ pkgs.lib.optionals (!isDarwin) [
              pkgs.wayland
              pkgs.libxkbcommon
              pkgs.vulkan-loader
              pkgs.xorg.libX11
              pkgs.xorg.libXcursor
              pkgs.xorg.libXrandr
              pkgs.xorg.libXi
            ];
        in
        {
          cargo-check = pkgs.rustPlatform.buildRustPackage {
            pname = "halloy-check";
            version = builtins.replaceStrings [ "\n" ] [ "" ] (builtins.readFile ./VERSION);
            src = pkgs.lib.cleanSource ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };

            inherit nativeBuildInputs buildInputs;

            # Only run cargo check, don't build
            buildPhase = ''
              cargo check --release
            '';

            installPhase = ''
              mkdir -p $out
              touch $out/ok
            '';

            doCheck = false;
          };
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };

          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
            ];
          };

          isDarwin = pkgs.stdenv.isDarwin;
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs = [
              rustToolchain
              pkgs.pkg-config
              pkgs.cmake
            ];

            buildInputs =
              [
                pkgs.openssl
                pkgs.xz
              ]
              ++ pkgs.lib.optionals isDarwin [
                pkgs.apple-sdk_15
              ]
              ++ pkgs.lib.optionals (!isDarwin) [
                pkgs.wayland
                pkgs.libxkbcommon
                pkgs.vulkan-loader
                pkgs.xorg.libX11
                pkgs.xorg.libXcursor
                pkgs.xorg.libXrandr
                pkgs.xorg.libXi
              ];
          };
        }
      );
    };
}
