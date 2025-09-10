// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
/// src-tauri/src/main.rs
// 宣告 `error` 和 `scraper` 模組
mod error;
mod scraper;

use crate::config::AppConfig;
use error::Error;
use futures::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
// (修正) 引入 Emitter trait，並整理 AppHandle
use tauri::{AppHandle, Emitter};

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
pub struct AnalysisResult {
    highlighted_data: Vec<UserReportData>,
    normal_data: Vec<UserReportData>,
}

#[derive(Serialize, Clone)]
struct ProgressPayload {
    current: usize,
    total: usize,
    user_id: String,
}

// --- Tauri 命令 (Tauri Command) ---
#[tauri::command]
async fn analyze_ptt_article(
    // (修正) 使用引入作用域的 AppHandle
    app: AppHandle,
    url: String,
    filter_types: Vec<String>,
    keywords: Option<Vec<String>>,
    highlight_condition: Option<String>,
) -> Result<AnalysisResult, Error> {
    println!(
        "接收到分析請求: url={}, types={:?}, keywords={:?}, highlight={:?}",
        url, filter_types, keywords, highlight_condition
    );

    // 步驟 1: 讀取設定檔
    let config = config::load_config();
    println!("讀取設定檔完成: {:?}", config);

    // 步驟 2: 呼叫爬蟲模組來爬取 PTT 文章頁面
    let article_data = scraper::scrape_ptt_article(&url, &filter_types, &keywords).await?;
    println!(
        "文章爬取完成: '{}', 看板: {}, 找到 {} 位符合條件的使用者。",
        article_data.title,
        article_data.board,
        article_data.user_comment_counts.len()
    );

    if article_data.user_comment_counts.is_empty() {
        return Ok(AnalysisResult {
            highlighted_data: vec![],
            normal_data: vec![],
        });
    }

    // 步驟 3: 併發查詢 pttweb.cc
    let users_to_scrape: Vec<(String, u32)> =
        article_data.user_comment_counts.into_iter().collect();
    let total_users = users_to_scrape.len();

    let report_data: Vec<UserReportData> = stream::iter(users_to_scrape)
        .enumerate()
        .map(|(i, (user, article_comments))| {
            let app_handle = app.clone();
            async move {
                let payload = ProgressPayload {
                    current: i + 1,
                    total: total_users,
                    user_id: user.clone(),
                };
                // (修正) emit 現在可以正常使用了
                app_handle.emit("SCRAPE_PROGRESS", payload).ok();

                match scraper::scrape_ptt_web(&user).await {
                    Ok(ptt_web_data) => UserReportData {
                        user,
                        article_comments,
                        board_comments: ptt_web_data.board_comments,
                        total_comments: ptt_web_data.total_comments,
                    },
                    Err(Error::PttWebUserNotFound(_)) => {
                        println!("警告: 在 pttweb.cc 查無使用者 '{}'，將使用預設值。", user);
                        UserReportData {
                            user,
                            article_comments,
                            board_comments: HashMap::new(),
                            total_comments: 0,
                        }
                    }
                    Err(e) => {
                        println!(
                            "錯誤: 查詢 '{}' 時發生非預期錯誤: {:?}，將使用預設值。",
                            user, e
                        );
                        UserReportData {
                            user,
                            article_comments,
                            board_comments: HashMap::new(),
                            total_comments: 0,
                        }
                    }
                }
            }
        })
        .buffer_unordered(10)
        .collect()
        .await;

    println!("所有使用者 pttweb.cc 資料查詢完成。");

    // 步驟 4: 資料處理 (排序與高亮)
    let sorted_data = sort_data(report_data, &config);
    let result = highlight_data(sorted_data, highlight_condition);

    println!("資料處理完成，回傳結果。");
    Ok(result)
}

fn sort_data(mut data: Vec<UserReportData>, config: &AppConfig) -> Vec<UserReportData> {
    let sort_by = &config.sorting.sort_by;
    let order = &config.sorting.order;

    data.sort_by(|a, b| {
        let val_a = match sort_by.as_str() {
            "本文留言數" => a.article_comments,
            "生涯總留言數" => a.total_comments,
            board_name => *a.board_comments.get(board_name).unwrap_or(&0),
        };
        let val_b = match sort_by.as_str() {
            "本文留言數" => b.article_comments,
            "生涯總留言數" => b.total_comments,
            board_name => *b.board_comments.get(board_name).unwrap_or(&0),
        };

        if *order == "desc" {
            val_b.cmp(&val_a)
        } else {
            val_a.cmp(&val_b)
        }
    });
    data
}

fn highlight_data(
    data: Vec<UserReportData>,
    highlight_condition: Option<String>,
) -> AnalysisResult {
    if let Some(condition) = highlight_condition {
        if let Some((board, op, threshold)) = parse_highlight_condition(&condition) {
            let (highlighted_data, normal_data) = data.into_iter().partition(|user| {
                let board_comments = user.board_comments.get(&board).cloned().unwrap_or(0) as f64;
                let total_comments = user.total_comments as f64;

                let value_to_compare = if threshold.is_percentage && total_comments > 0.0 {
                    (board_comments / total_comments) * 100.0
                } else {
                    board_comments
                };

                match op.as_str() {
                    ">" => value_to_compare > threshold.value,
                    ">=" => value_to_compare >= threshold.value,
                    "<" => value_to_compare < threshold.value,
                    "<=" => value_to_compare <= threshold.value,
                    "==" => (value_to_compare - threshold.value).abs() < f64::EPSILON,
                    _ => false,
                }
            });
            return AnalysisResult {
                highlighted_data,
                normal_data,
            };
        }
    }

    AnalysisResult {
        highlighted_data: vec![],
        normal_data: data,
    }
}

struct Threshold {
    value: f64,
    is_percentage: bool,
}

fn parse_highlight_condition(condition: &str) -> Option<(String, String, Threshold)> {
    let parts: Vec<&str> = condition.split(',').map(|s| s.trim()).collect();
    if parts.len() != 3 {
        return None;
    }

    let board = parts[0].to_string();
    let op = parts[1].to_string();

    let value_str = parts[2];
    let is_percentage = value_str.ends_with('%');
    let number_part = if is_percentage {
        &value_str[..value_str.len() - 1]
    } else {
        value_str
    };

    if let Ok(value) = number_part.parse::<f64>() {
        Some((
            board,
            op,
            Threshold {
                value,
                is_percentage,
            },
        ))
    } else {
        None
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![analyze_ptt_article])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
