{
  description = "Halloy - IRC client";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
    }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);

      version = builtins.replaceStrings [ "\n" ] [ "" ] (builtins.readFile ./VERSION);

      perSystem = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
          };

          craneLib = crane.mkLib pkgs;

          isDarwin = pkgs.stdenv.isDarwin;

          nativeBuildInputs = [
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

          commonArgs = {
            # The build embeds fonts, sounds, and theme assets outside the
            # default cargo file set, so keep the broader project source filter.
            src = pkgs.lib.cleanSource ./.;
            pname = "halloy";
            inherit version nativeBuildInputs buildInputs;
            strictDeps = true;
            doCheck = false;

            meta = {
              description = "Halloy IRC Client";
              homepage = "https://halloy.chat";
              license = pkgs.lib.licenses.gpl3Plus;
              mainProgram = "halloy";
            };
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          halloy = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
          });

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

          cargoCheck = craneLib.mkCargoDerivation (commonArgs // {
            pname = "halloy-check";
            inherit cargoArtifacts;
            doInstallCargoArtifacts = false;

            buildPhaseCargoCommand = ''
              cargo check --release --locked
            '';

            installPhaseCommand = ''
              mkdir -p $out
              touch $out/ok
            '';
          });

          devShell = craneLib.devShell {
            inherit nativeBuildInputs buildInputs;

            packages = [
              pkgs.rust-analyzer
            ];

            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          };
        in
        {
          inherit pkgs isDarwin halloy halloyApp cargoCheck devShell;
        }
      );
    in
    {
      packages = nixpkgs.lib.mapAttrs (
        _: attrs:
        {
          default = if attrs.isDarwin then attrs.halloyApp else attrs.halloy;
          inherit (attrs) halloy;
        }
        // attrs.pkgs.lib.optionalAttrs attrs.isDarwin { app = attrs.halloyApp; }
      ) perSystem;

      checks = nixpkgs.lib.mapAttrs (_: attrs: { cargo-check = attrs.cargoCheck; }) perSystem;

      devShells = nixpkgs.lib.mapAttrs (_: attrs: { default = attrs.devShell; }) perSystem;
    };
}
