// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod error;
mod scraper;

// (新增) 引入 AppConfig 以在 Payload 中使用
use config::AppConfig;
use error::Error;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::Emitter;

const CONCURRENT_LIMIT: usize = 10;

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

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    metadata: ReportMetadata,
    highlighted_data: Vec<UserReportData>,
    normal_data: Vec<UserReportData>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReportMetadata {
    title: String,
    url: String,
    board: String,
    filter_types: Vec<String>,
    keywords: Option<Vec<String>>,
    highlight_condition: Option<String>,
}

// --- Tauri 事件 Payload ---
#[derive(Serialize, Deserialize, Debug, Clone)]
struct ProgressPayload {
    current: usize,
    total: usize,
    user_id: String,
}

// (新增) 定義一個結構體來接收來自前端的完整 payload
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnalyzePayload {
    url: String,
    filter_types: Vec<String>,
    keywords: Option<Vec<String>>,
    highlight_condition: Option<String>,
    config: AppConfig, // 包含前端傳來的設定
}

// --- Tauri 命令 (Tauri Command) ---
#[tauri::command]
async fn analyze_ptt_article(
    app: tauri::AppHandle,
    // (修改) 整個 command 的參數改為接收單一的 payload
    payload: AnalyzePayload,
) -> Result<AnalysisResult, Error> {
    // (修改) 從 payload 中解構出所需變數
    let url = payload.url;
    let filter_types = payload.filter_types;
    let keywords = payload.keywords;
    let highlight_condition = payload.highlight_condition;
    let app_config = payload.config; // 直接使用從前端傳來的設定

    // 步驟 1: 爬取 PTT 文章頁面
    let article_data = scraper::scrape_ptt_article(&url, &filter_types, &keywords).await?;

    if article_data.user_comment_counts.is_empty() {
        let metadata = ReportMetadata {
            title: article_data.title,
            url,
            board: article_data.board,
            filter_types,
            keywords,
            highlight_condition,
        };
        return Ok(AnalysisResult {
            metadata,
            highlighted_data: vec![],
            normal_data: vec![],
        });
    }

    // (修改) 直接使用來自 payload 的設定，不再從檔案載入
    // let app_config = config::load_config(&app); // <--- 移除此行
    let mut target_boards = app_config.boards.clone(); // 使用傳入的看板列表
    if !target_boards.contains(&article_data.board) {
        target_boards.push(article_data.board.clone());
    }

    // 步驟 2: 併發查詢 pttweb.cc
    let users_to_scrape: Vec<_> = article_data.user_comment_counts.keys().cloned().collect();
    let total_users = users_to_scrape.len();

    let report_futures = stream::iter(users_to_scrape.into_iter().enumerate())
        .map(|(i, user)| {
            let app_handle = app.clone();
            let target_boards_clone = target_boards.clone();
            async move {
                let payload = ProgressPayload {
                    current: i + 1,
                    total: total_users,
                    user_id: user.clone(),
                };
                let _ = app_handle.emit("SCRAPE_PROGRESS", payload);

                match scraper::scrape_ptt_web(&user, &target_boards_clone).await {
                    Ok(ptt_web_data) => (user, Some(ptt_web_data)),
                    Err(Error::PttWebUserNotFound(_)) => (user, None),
                    Err(e) => {
                        println!("查詢 {} 時發生非預期錯誤: {:?}", user, e);
                        (user, None)
                    }
                }
            }
        })
        .buffer_unordered(CONCURRENT_LIMIT);

    let ptt_web_results: Vec<_> = report_futures.collect().await;

    let mut report_data: Vec<UserReportData> = ptt_web_results
        .into_iter()
        .map(|(user, ptt_web_data_option)| {
            let (board_comments, total_comments) = ptt_web_data_option
                .map(|data| (data.board_comments, data.total_comments))
                .unwrap_or_else(|| (HashMap::new(), 0));

            UserReportData {
                user: user.clone(),
                article_comments: *article_data.user_comment_counts.get(&user).unwrap_or(&0),
                board_comments,
                total_comments,
            }
        })
        .collect();

    // 步驟 3: 排序資料
    report_data.sort_by(|a, b| {
        let val_a: u32;
        let val_b: u32;

        match app_config.sorting.sort_by.as_str() {
            "本文留言數" => {
                val_a = a.article_comments;
                val_b = b.article_comments;
            }
            "生涯總留言數" => {
                val_a = a.total_comments;
                val_b = b.total_comments;
            }
            board_name => {
                val_a = *a.board_comments.get(board_name).unwrap_or(&0);
                val_b = *b.board_comments.get(board_name).unwrap_or(&0);
            }
        }

        if app_config.sorting.order == "desc" {
            val_b.cmp(&val_a)
        } else {
            val_a.cmp(&val_b)
        }
    });

    // 步驟 4: 處理高亮邏輯
    let (highlighted_data, normal_data) = if let Some(condition) =
        highlight_condition.as_ref().filter(|s| !s.is_empty())
    {
        let parts: Vec<&str> = condition.split(',').collect();
        if parts.len() == 3 {
            let hl_board = parts[0].trim();
            let operator = parts[1].trim();
            let value_str = parts[2].trim();
            let is_percentage = value_str.ends_with('%');
            let threshold = value_str
                .trim_end_matches('%')
                .parse::<f64>()
                .unwrap_or(-1.0);

            if threshold >= 0.0 {
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
                        "==" => (value_to_compare - threshold).abs() < 1e-9,
                        _ => false,
                    }
                })
            } else {
                (vec![], report_data)
            }
        } else {
            (vec![], report_data)
        }
    } else {
        (vec![], report_data)
    };

    let metadata = ReportMetadata {
        title: article_data.title,
        url,
        board: article_data.board,
        filter_types,
        keywords,
        highlight_condition,
    };

    Ok(AnalysisResult {
        metadata,
        highlighted_data,
        normal_data,
    })
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![analyze_ptt_article])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
