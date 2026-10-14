use std::env;
use std::path::PathBuf;

fn main() {
    // Windows のダイアログ（tauri-plugin-dialog）はコモンコントロール v6 を使うので、アプリのマニフェストが要る。
    // tauri-build の既定はアプリ本体の実行ファイルにだけマニフェストを埋め込むため、`cargo test` の実行ファイルが
    // STATUS_ENTRYPOINT_NOT_FOUND で起動できない。既定の埋め込みを切り、リンカの引数で全ターゲットに埋め込む
    // （Tauri のサンプルアプリと同じ方法）。
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("tauri-build に失敗した");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
