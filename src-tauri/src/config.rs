// src-tauri/src/config.rs

use serde::{Deserialize, Serialize};

// (新增) 也加上 Serialize，讓這個結構體可以在 Rust 端與前端之間雙向傳遞
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SortingConfig {
    pub sort_by: String,
    pub order: String, // "asc" or "desc"
}

// (新增) 也加上 Serialize
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub boards: Vec<String>,
    pub sorting: SortingConfig,
}

// Default 實作依然有用，可以作為前端初始狀態的參考
impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            boards: vec!["Gossiping".to_string(), "HatePolitics".to_string()],
            sorting: SortingConfig {
                sort_by: "本文留言數".to_string(),
                order: "desc".to_string(),
            },
        }
    }
}
