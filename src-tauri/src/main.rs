// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
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

// --- 用於前端進度事件的 Payload ---
#[derive(Clone, serde::Serialize)]
struct ProgressPayload {
    current: usize,
    total: usize,
    user_id: String,
}

// --- TASK-204 新增: 最終回傳給前端的資料結構 ---
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnalysisResult {
    highlighted_data: Vec<UserReportData>,
    normal_data: Vec<UserReportData>,
}

// --- Tauri 命令 (Tauri Command) - TASK-204 最終版 ---
#[tauri::command]
async fn analyze_ptt_article(
    app: tauri::AppHandle,
    url: String,
    filter_type: String,
    keyword: Option<String>,
    highlight_condition: Option<String>, // <-- 新增 highlight 參數
) -> Result<AnalysisResult, Error> {
    const CONCURRENT_LIMIT: usize = 10;

    // --- 步驟 1: 爬取 PTT 文章頁面 ---
    let article_data = scraper::scrape_ptt_article(&url, &filter_type, &keyword).await?;
    let total_users = article_data.user_comment_counts.len();
    println!(
        "文章爬取完成: '{}', 看板: {}, 找到 {} 位符合條件的使用者。",
        article_data.title, article_data.board, total_users
    );

    // --- 步驟 2: 併發查詢 pttweb.cc ---
    println!("開始深度查詢 {} 位使用者的 pttweb.cc 資料...", total_users);
    let user_stream = stream::iter(article_data.user_comment_counts.into_iter());
    let tasks = user_stream
        .enumerate()
        .map(|(i, (user, article_comments))| {
            let app_handle = app.clone();
            async move {
                app_handle
                    .emit(
                        "SCRAPE_PROGRESS",
                        ProgressPayload {
                            current: i + 1,
                            total: total_users,
                            user_id: user.clone(),
                        },
                    )
                    .unwrap();
                match scraper::scrape_ptt_web(&user).await {
                    Ok(ptt_web_data) => Some(UserReportData {
                        user,
                        article_comments,
                        board_comments: ptt_web_data.board_comments,
                        total_comments: ptt_web_data.total_comments,
                    }),
                    Err(Error::PttWebUserNotFound(user_id)) => {
                        println!("⚠️ 查無使用者: {}, 將回傳預設值。", user_id);
                        Some(UserReportData {
                            user,
                            article_comments,
                            board_comments: HashMap::new(),
                            total_comments: 0,
                        })
                    }
                    Err(e) => {
                        println!("❌ 查詢 {} 時發生錯誤: {:?}，將忽略此使用者。", user, e);
                        None
                    }
                }
            }
        });
    let mut report_data: Vec<UserReportData> = tasks
        .buffer_unordered(CONCURRENT_LIMIT)
        .filter_map(|res| async { res })
        .collect()
        .await;

    println!("✅ pttweb.cc 資料查詢完成。");
    println!("⏳ 開始進行資料排序與篩選...");

    // --- 步驟 3: 讀取設定檔並排序資料 ---
    let config = config::load_config();
    report_data.sort_by(|a, b| {
        let val_a = match config.sorting.sort_by.as_str() {
            "本文留言數" => a.article_comments,
            "生涯總留言數" => a.total_comments,
            board_name => *a.board_comments.get(board_name).unwrap_or(&0),
        };
        let val_b = match config.sorting.sort_by.as_str() {
            "本文留言數" => b.article_comments,
            "生涯總留言數" => b.total_comments,
            board_name => *b.board_comments.get(board_name).unwrap_or(&0),
        };
        if config.sorting.order == "asc" {
            val_a.cmp(&val_b)
        } else {
            val_b.cmp(&val_a)
        }
    });

    // --- 步驟 4: 處理高亮邏輯 ---
    let (highlighted_data, normal_data) = if let Some(condition) =
        highlight_condition.filter(|c| !c.is_empty())
    {
        let parts: Vec<&str> = condition.split(',').collect();
        if parts.len() == 3 {
            let hl_board = parts[0];
            let operator = parts[1];
            let value_str = parts[2];
            let is_percentage = value_str.contains('%');
            let threshold = value_str.replace('%', "").parse::<f64>().unwrap_or(0.0);

            let (highlighted, normal): (Vec<_>, Vec<_>) =
                report_data.into_iter().partition(|user| {
                    let board_comments = *user.board_comments.get(hl_board).unwrap_or(&0) as f64;
                    let total_comments = user.total_comments as f64;
                    let value_to_compare = if is_percentage && total_comments > 0.0 {
                        (board_comments / total_comments) * 100.0
                    } else {
                        board_comments
                    };
                    match operator {
                        "<" => value_to_compare < threshold,
                        "<=" => value_to_compare <= threshold,
                        ">" => value_to_compare > threshold,
                        ">=" => value_to_compare >= threshold,
                        "==" => (value_to_compare - threshold).abs() < f64::EPSILON,
                        _ => false,
                    }
                });
            (highlighted, normal)
        } else {
            (vec![], report_data) // 條件格式錯誤，全部視為一般
        }
    } else {
        (vec![], report_data) // 無高亮條件，全部視為一般
    };

    println!("✅ 資料處理完成。");
    Ok(AnalysisResult {
        highlighted_data,
        normal_data,
    })
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![analyze_ptt_article])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
