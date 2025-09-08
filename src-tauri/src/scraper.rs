use crate::error::Error;
use scraper::{Html, Selector};
use std::collections::HashMap;

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
