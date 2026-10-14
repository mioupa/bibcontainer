// リリースビルドの Windows でコンソールウィンドウを出さない。消さないこと。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    bibcontainer_lib::run();
}
