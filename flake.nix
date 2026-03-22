{
  description = "Azimuth-ID Auth Ecosystem - Rust microservices";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" "llvm-tools-preview" ];
          targets = [ "x86_64-unknown-linux-musl" ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          name = "azimuth-dev";
          
          buildInputs = [
            rustToolchain
            pkgs.pkg-config
            pkgs.openssl
            pkgs.postgresql
            pkgs.protobuf      # для генерации gRPC кода
            pkgs.cargo-watch   # hot-reload при разработке
            pkgs.sqlx-cli      # миграции БД
          ];
          
          # Переменные окружения
          env = {
            # Не вендорить openssl — использовать системный
            OPENSSL_NO_VENDOR = "1";
            PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
            
            # SQLx оффлайн-режим (не нужна БД при компиляции)
            SQLX_OFFLINE = "true";
          };
          
          shellHook = ''
            echo "🧭 Azimuth-ID dev environment ready!"
            echo "   • rustc: $(rustc --version)"
            echo "   • cargo: $(cargo --version)"
            echo "   • protobuf: ${pkgs.protobuf.version}"
            echo ""
            echo "Полезные команды:"
            echo "  • cargo watch -x run -p azimuth-id"
            echo "  • sqlx migrate run --source services/azimuth-id/migrations"
            echo "  • cargo test --workspace"
          '';
        };

        # Пакет для сборки сервиса
        packages.azimuth-id = pkgs.rustPlatform.buildRustPackage {
          pname = "azimuth-id";
          version = "0.1.0";
          src = ./services/azimuth-id;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.pkg-config pkgs.protobuf ];
          buildInputs = [ pkgs.openssl pkgs.postgresql ];
        };
      }
    );
}
