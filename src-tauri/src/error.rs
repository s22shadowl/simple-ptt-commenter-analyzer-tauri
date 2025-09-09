use serde::Serialize;
use thiserror::Error;

/// 定義應用程式的統一錯誤類型。
#[derive(Debug, Error)]
pub enum Error {
    /// 代表 reqwest 網路請求過程中發生的任何錯誤。
    #[error("網路請求失敗: {0}")]
    Request(#[from] reqwest::Error),

    /// 當在 pttweb.cc 找不到指定使用者時回傳。
    #[error("在 pttweb.cc 找不到使用者: {0}")]
    PttWebUserNotFound(String),

    /// 當解析 pttweb.cc 的 HTML 結構失敗或格式不符預期時回傳。
    #[error("解析 pttweb.cc HTML 失敗: {0}")]
    PttWebParse(String),
}

// 為了讓錯誤可以被序列化並傳遞到前端，我們需要手動為 Error 實現 Serialize trait。
// 這樣在 Tauri 命令回傳 Result<T, Error> 時，前端才能正確接收到錯誤訊息。
impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
