/// src-tauri/src/scraper.rs
// --- 新增的 use ---
use crate::{error::Error, PttWebData}; // 從 main.rs 引入 PttWebData
use once_cell::sync::Lazy;
use regex::Regex;

// --- 原有的 use ---
use scraper::{Html, Selector};
use std::collections::HashMap;

// --- 全域靜態變數 ---
// 使用 once_cell::sync::Lazy 來確保 Regex 只被編譯一次，提升效能。
// 這個 Regex 嚴格遵循 src/index.ts 的邏輯，用於從 "..., 共 XXX 則" 的字串中提取數字。
static TOTAL_COMMENTS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r", 共(\d+)則").unwrap());

/// 用於儲存 `scrape_ptt_article` 函式爬取結果的結構。
#[derive(Debug)]
pub struct PttArticleData {
    pub user_comment_counts: HashMap<String, u32>,
    pub board: String,
    pub title: String,
}

/// (對應 TS: scrapePttArticle) 爬取指定 PTT 文章，篩選並統計留言者。
pub async fn scrape_ptt_article(
    url: &str,
    filter_type: &str,
    keyword: &Option<String>,
) -> Result<PttArticleData, Error> {
    // 1. 建立 reqwest 客戶端並設定 "over18" cookie
    let client = reqwest::Client::new();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::COOKIE,
        reqwest::header::HeaderValue::from_static("over18=1"),
    );

    // 2. 發送網路請求並取得 HTML 內容
    let html = client
        .get(url)
        .headers(headers)
        .send()
        .await?
        .text()
        .await?;

    // 3. 使用 `scraper` crate 解析 HTML
    let document = Html::parse_document(&html);

    // 4. 定義 CSS 選擇器，這一步若失敗會導致程式 panic，因為這是程式的核心邏輯
    let title_selector = Selector::parse(".article-metaline .article-meta-value").unwrap();
    let board_selector = Selector::parse(".article-metaline-right .article-meta-value").unwrap();
    let push_selector = Selector::parse(".push").unwrap();
    let tag_selector = Selector::parse(".push-tag").unwrap();
    let user_selector = Selector::parse(".push-userid").unwrap();
    let content_selector = Selector::parse(".push-content").unwrap();

    // 5. 提取文章標題與看板名稱
    let title = document
        .select(&title_selector)
        .nth(2) // 根據技術文件，標題是第三個符合的元素
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "未知標題".to_string());

    let board = document
        .select(&board_selector)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    // 6. 遍歷所有留言區塊，進行篩選與統計
    let mut user_comment_counts = HashMap::new();
    for element in document.select(&push_selector) {
        let tag_text = element
            .select(&tag_selector)
            .next()
            .map(|t| t.text().collect::<String>())
            .unwrap_or_default();
        let user = element
            .select(&user_selector)
            .next()
            .map(|u| u.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let content_raw = element
            .select(&content_selector)
            .next()
            .map(|c| c.text().collect::<String>())
            .unwrap_or_default();

        // 如果缺少使用者 ID 或留言內容，則跳過此筆留言
        if user.is_empty() || content_raw.is_empty() {
            continue;
        }

        // 清理留言內容，移除前方的冒號與空白
        let content = content_raw
            .trim_start_matches(|c: char| c == ':' || c.is_whitespace())
            .to_string();

        let comment_type = if tag_text.contains('推') {
            "push"
        } else if tag_text.contains('噓') {
            "hate"
        } else if tag_text.contains('→') {
            "arrow"
        } else {
            "unknown"
        };

        // 判斷是否符合篩選條件
        let type_match = filter_type == "all" || filter_type == comment_type;
        let keyword_match = keyword.as_ref().map_or(true, |k| content.contains(k));

        if type_match && keyword_match {
            *user_comment_counts.entry(user).or_insert(0) += 1;
        }
    }

    Ok(PttArticleData {
        user_comment_counts,
        board,
        title,
    })
}

// --- TASK-202 新增函式 (已重構) ---
/// (對應 TS: scrapePttWeb) 爬取 pttweb.cc 取得使用者留言統計。
pub async fn scrape_ptt_web(user_id: &str) -> Result<PttWebData, Error> {
    // 1. 建構請求 URL
    let url = format!("https://www.pttweb.cc/user/{}?t=message", user_id);

    // 2. 發送請求並解析 HTML
    let html = reqwest::get(&url).await?.text().await?;
    let document = Html::parse_document(&html);

    // 3. 邊界檢查: 檢查使用者是否存在
    let title_selector = Selector::parse("title").unwrap();
    if let Some(title_element) = document.select(&title_selector).next() {
        if title_element
            .text()
            .collect::<String>()
            .contains("沒有此作者")
        {
            return Err(Error::PttWebUserNotFound(user_id.to_string()));
        }
    }

    // 4. 提取生涯總留言數 (修正部分)
    let headline_selector = Selector::parse("div.headline").unwrap();
    // --- 步驟 4.1: 先提取 headline 的文字到一個具備所有權的 String 變數 ---
    let headline_text = document
        .select(&headline_selector)
        .next()
        .map(|el| el.text().collect::<String>());

    // --- 步驟 4.2: 對這個 String 變數進行操作 ---
    let total_comments = headline_text
        .as_ref() // 從 Option<String> 借用 &String
        .and_then(|text| TOTAL_COMMENTS_RE.captures(text)) // 對 &String 進行 regex 匹配
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .ok_or_else(|| {
            Error::PttWebParse(format!("無法從 headline 解析 {} 的總留言數", user_id))
        })?;

    // 5. 提取各看板留言數 (重構後)
    let board_box_selector = Selector::parse(".e7-wrapper-board .e7-box").unwrap();
    let board_name_selector = Selector::parse("a").unwrap();
    let board_count_selector = Selector::parse("span.ml-2").unwrap();

    let board_comments: HashMap<String, u32> = document
        .select(&board_box_selector)
        .filter_map(|element| {
            // 提取看板名稱
            let name = element
                .select(&board_name_selector)
                .next()?
                .text()
                .collect::<String>();
            let name = name.trim();

            // 如果名稱為空，則跳過此元素
            if name.is_empty() {
                return None;
            }

            // 提取留言數
            let count_text = element
                .select(&board_count_selector)
                .next()?
                .text()
                .collect::<String>();

            let count = count_text.trim().parse::<u32>().ok()?;

            // 回傳 Some((看板名稱, 留言數))，filter_map 會收集所有 Some 的結果
            Some((name.to_string(), count))
        })
        .collect(); // 將 iterator 的結果收集成一個 HashMap

    // 6. 回傳結果
    Ok(PttWebData {
        board_comments,
        total_comments,
    })
}
