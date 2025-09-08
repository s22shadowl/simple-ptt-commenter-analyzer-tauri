// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// 宣告 `error` 和 `scraper` 模組
mod error;
mod scraper;

use error::Error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

// --- Tauri 命令 (Tauri Command) ---
#[tauri::command]
async fn analyze_ptt_article(
    url: String,
    filter_type: String,
    keyword: Option<String>,
) -> Result<Vec<UserReportData>, Error> {
    println!(
        "接收到分析請求: url={}, type={}, keyword={:?}",
        url, filter_type, keyword
    );

    // 步驟 1: 呼叫爬蟲模組來爬取 PTT 文章頁面
    let article_data = scraper::scrape_ptt_article(&url, &filter_type, &keyword).await?;

    println!(
        "文章爬取完成: '{}', 看板: {}, 找到 {} 位符合條件的使用者。",
        article_data.title,
        article_data.board,
        article_data.user_comment_counts.len()
    );

    // TODO: 在後續任務中，會在這裡加入併發查詢 pttweb.cc 的邏輯。

    // 暫時將第一階段的爬取結果轉換成最終報告格式。
    // `board_comments` 和 `total_comments` 暫時為空。
    let report_data: Vec<UserReportData> = article_data
        .user_comment_counts
        .into_iter()
        .map(|(user, article_comments)| UserReportData {
            user,
            article_comments,
            board_comments: HashMap::new(),
            total_comments: 0,
        })
        .collect();

    Ok(report_data)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![analyze_ptt_article])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
