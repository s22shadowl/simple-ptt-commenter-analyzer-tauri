// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// src-tauri/src/main.rs
// 宣告 `error` 和 `scraper` 模組
mod error;
mod scraper;

use error::Error;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::Emitter;

// --- 核心資料結構 (Core Data Structures) ---
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PttWebData {
    board_comments: HashMap<String, u32>,
    total_comments: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserReportData {
    user: String,
    article_comments: u32,
    board_comments: HashMap<String, u32>,
    total_comments: u32,
}

// --- 新增：用於前端進度事件的 Payload ---
#[derive(Clone, serde::Serialize)]
struct ProgressPayload {
    current: usize,
    total: usize,
    user_id: String,
}

// --- Tauri 命令 (Tauri Command) - TASK-203 更新版 ---
#[tauri::command]
async fn analyze_ptt_article(
    app: tauri::AppHandle, // <-- AppHandle 本身就具備 emit 方法
    url: String,
    filter_type: String,
    keyword: Option<String>,
) -> Result<Vec<UserReportData>, Error> {
    const CONCURRENT_LIMIT: usize = 10; // 設定併發查詢的上限

    // --- 步驟 1: 爬取 PTT 文章頁面，與之前相同 ---
    let article_data = scraper::scrape_ptt_article(&url, &filter_type, &keyword).await?;
    let total_users = article_data.user_comment_counts.len();
    println!(
        "文章爬取完成: '{}', 看板: {}, 找到 {} 位符合條件的使用者。",
        article_data.title, article_data.board, total_users
    );

    // --- 步驟 2: 併發查詢 pttweb.cc ---
    println!("開始深度查詢 {} 位使用者的 pttweb.cc 資料...", total_users);

    // 將使用者資料轉換為非同步流
    let user_stream = stream::iter(article_data.user_comment_counts.into_iter());

    let tasks = user_stream
        .enumerate()
        .map(|(i, (user, article_comments))| {
            let app_handle = app.clone();
            async move {
                // 發送進度事件給前端 (修正: emit_all -> emit)
                app_handle
                    .emit(
                        "SCRAPE_PROGRESS",
                        ProgressPayload {
                            current: i + 1,
                            total: total_users,
                            user_id: user.clone(),
                        },
                    )
                    .unwrap(); // 在此處 unwrap，因為事件發送失敗是嚴重問題

                // 執行 pttweb.cc 的爬取
                match scraper::scrape_ptt_web(&user).await {
                    Ok(ptt_web_data) => Some(UserReportData {
                        user,
                        article_comments,
                        board_comments: ptt_web_data.board_comments,
                        total_comments: ptt_web_data.total_comments,
                    }),
                    // 如果只是查無使用者，則回傳預設值，讓流程繼續
                    Err(Error::PttWebUserNotFound(user_id)) => {
                        println!("⚠️ 查無使用者: {}, 將回傳預設值。", user_id);
                        Some(UserReportData {
                            user,
                            article_comments,
                            board_comments: HashMap::new(),
                            total_comments: 0,
                        })
                    }
                    // 對於其他更嚴重的錯誤，則印出日誌並忽略此使用者
                    Err(e) => {
                        println!("❌ 查詢 {} 時發生錯誤: {:?}，將忽略此使用者。", user, e);
                        None
                    }
                }
            }
        });

    // 使用 buffer_unordered 進行併發處理，並收集結果
    let report_data: Vec<UserReportData> = tasks
        .buffer_unordered(CONCURRENT_LIMIT)
        .filter_map(|res| async { res }) // 過濾掉查詢失敗的 None 結果
        .collect()
        .await;

    println!("✅ pttweb.cc 資料查詢完成。");
    Ok(report_data)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![analyze_ptt_article])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
