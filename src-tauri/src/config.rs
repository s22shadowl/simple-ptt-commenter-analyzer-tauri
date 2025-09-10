use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

/// 對應 config.json 中的 "sorting" 物件
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SortingConfig {
    pub sort_by: String,
    pub order: String, // "asc" or "desc"
}

/// 對應 config.json 的頂層結構
#[derive(Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub boards: Vec<String>,
    pub sorting: SortingConfig,
}

/// 提供一個預設的 AppConfig，用於 config.json 不存在或解析失敗時的回退
impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            boards: vec![],
            sorting: SortingConfig {
                sort_by: "本文留言數".to_string(),
                order: "desc".to_string(),
            },
        }
    }
}

/// 讀取並解析 config.json 檔案。
///
/// 此函式會嘗試讀取應用程式執行目錄下的 "config.json"。
/// 若檔案不存在、無法讀取、或 JSON 格式錯誤，將會印出警告並回傳預設設定。
pub fn load_config() -> AppConfig {
    let config_path = PathBuf::from("config.json");
    if let Ok(file_content) = fs::read_to_string(config_path) {
        match serde_json::from_str(&file_content) {
            Ok(config) => config,
            Err(e) => {
                println!("⚠️ 解析 config.json 失敗: {}，將使用預設設定。", e);
                AppConfig::default()
            }
        }
    } else {
        println!("⚠️ 找不到 config.json，將使用預設設定。");
        AppConfig::default()
    }
}
