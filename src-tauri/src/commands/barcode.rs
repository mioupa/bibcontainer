//! バーコードの判定とCコード表（DB には触らない。計算だけなので同期のコマンド）。

use bibcontainer_core::barcode::{self, CCodeInfo, CCodeTable, ScanCode};

/// 読み取った文字列を、ISBN-13・2段目・雑誌・一般 JAN・不正のどれかに判定する。
/// ISBN-10 は ISBN-13 に変換して返す。
#[tauri::command]
pub fn classify_code(raw: String) -> ScanCode {
    barcode::classify(&raw)
}

/// Cコード表（付録 A）。
#[tauri::command]
pub fn c_code_table() -> CCodeTable {
    barcode::c_code_table()
}

/// Cコード（4 桁）の各桁の名称。4 桁の数字でなければ `null`。
#[tauri::command]
pub fn describe_c_code(code: String) -> Option<CCodeInfo> {
    barcode::describe_c_code(&code)
}
