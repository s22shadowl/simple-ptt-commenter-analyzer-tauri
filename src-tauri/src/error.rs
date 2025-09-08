use serde::Serialize;

/// 自訂的錯誤類型 Enum，用於統一處理應用程式中所有可能的錯誤。
#[derive(Debug, thiserror::Error, Serialize)]
pub enum Error {
    #[error("網路請求失敗: {0}")]
    Reqwest(String),

    #[error("HTML 解析失敗: {0}")]
    Parse(String),

    #[error("在頁面中找不到必要的內容: {0}")]
    ContentMissing(String),
}

/// 讓 `reqwest` 函式庫產生的錯誤可以被輕易轉換成我們自訂的 Error::Reqwest。
impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Error::Reqwest(err.to_string())
    }
}
