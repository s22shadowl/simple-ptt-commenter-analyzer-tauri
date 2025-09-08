// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- 核心資料結構 (Core Data Structures) ---
// 根據 TECHNICAL_DOCUMENTATION.md 建立，用於整個應用程式的資料傳遞。

/// pttweb.cc 查詢結果的結構。
/// `#[derive(...)]` 是一個屬性宏，自動為 struct 實現常用的 traits (特徵)。
/// - `Serialize`, `Deserialize`: 讓 struct 能夠與 JSON 格式互相轉換 (Serde)。
/// - `Debug`: 允許我們使用 `println!("{:?}", ...)` 來印出 struct 內容，方便除錯。
/// - `Clone`: 允許我們複製這個 struct 的實例。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PttWebData {
    board_comments: HashMap<String, u32>,
    total_comments: u32,
}

/// 最終報告中，單筆使用者資料的完整結構。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserReportData {
    user: String,
    article_comments: u32,
    board_comments: HashMap<String, u32>,
    total_comments: u32,
}

/// 我們主要的 Tauri 命令 (Command)。
/// - `@tauri::command`: 將這個 Rust 函式標記為可從前端 JavaScript 呼叫的命令。
/// - `async`: 表示這是一個非同步函式，允許在其中執行耗時操作 (如網路請求) 而不阻塞 UI。
/// - 回傳 `Result<Vec<UserReportData>, String>`:
///   - `Ok(Vec<UserReportData>)`: 成功時，直接回傳一個包含多筆使用者資料的 Vec (Vector)。Tauri 會自動將其序列化為 JSON 陣列。
///   - `Err(String)`: 失敗時，回傳一個錯誤訊息字串。
#[tauri::command]
async fn analyze_ptt_article(
    url: String,
    filter_type: String,
    keyword: Option<String>,
) -> Result<Vec<UserReportData>, String> {
    // 暫時的佔位符邏輯。
    // 在接下來的任務中，這裡將會被完整的爬蟲與分析邏輯取代。
    // 我們先模擬一個成功的回傳，以驗證資料結構和前後端通訊。
    println!(
        "接收到分析請求: url={}, type={}, keyword={:?}",
        url, filter_type, keyword
    );

    let mock_data = vec![
        UserReportData {
            user: "user_a".to_string(),
            article_comments: 5,
            board_comments: HashMap::from([
                ("Gossiping".to_string(), 100),
                ("C_Chat".to_string(), 50),
            ]),
            total_comments: 1000,
        },
        UserReportData {
            user: "user_b".to_string(),
            article_comments: 2,
            board_comments: HashMap::from([("Stock".to_string(), 250)]),
            total_comments: 500,
        },
    ];

    // 模擬一個網路延遲
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    Ok(mock_data)
}

fn main() {
    tauri::Builder::default()
        // `invoke_handler` 負責註冊所有你希望從前端呼叫的 Rust 命令。
        // `tauri::generate_handler![]` 是一個宏，會自動收集所有被 `#[tauri::command]` 標記的函式。
        .invoke_handler(tauri::generate_handler![analyze_ptt_article])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
