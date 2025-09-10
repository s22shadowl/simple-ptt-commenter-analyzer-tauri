use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Deserialize, Debug, Clone)]
pub struct SortingConfig {
    #[serde(default = "default_sort_by")]
    pub sort_by: String,
    #[serde(default = "default_order")]
    pub order: String, // "asc" or "desc"
}

#[derive(Deserialize, Debug, Clone)]
pub struct AppConfig {
    #[serde(default)]
    pub sorting: SortingConfig,
}

// --- serde default functions ---
fn default_sort_by() -> String {
    "本文留言數".to_string()
}
fn default_order() -> String {
    "desc".to_string()
}

// --- Default implementations ---
impl Default for SortingConfig {
    fn default() -> Self {
        SortingConfig {
            sort_by: default_sort_by(),
            order: default_order(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            sorting: SortingConfig::default(),
        }
    }
}

pub fn load_config() -> AppConfig {
    let path = Path::new("config.json");
    if path.exists() {
        fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    } else {
        AppConfig::default()
    }
}
