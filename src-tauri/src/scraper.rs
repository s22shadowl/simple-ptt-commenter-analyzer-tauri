use crate::error::Error;
use crate::PttWebData;
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{Html, Selector};
use std::collections::HashMap;
use tokio::time::{sleep, Duration};

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
    filter_types: &[String],
    keywords: &Option<Vec<String>>,
) -> Result<PttArticleData, Error> {
    let client = reqwest::Client::new();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::COOKIE,
        reqwest::header::HeaderValue::from_static("over18=1"),
    );

    let html = client
        .get(url)
        .headers(headers)
        .send()
        .await?
        .text()
        .await?;

    let document = Html::parse_document(&html);

    let title_selector = Selector::parse(".article-metaline .article-meta-value").unwrap();
    let board_selector = Selector::parse(".article-metaline-right .article-meta-value").unwrap();
    let push_selector = Selector::parse(".push").unwrap();
    let tag_selector = Selector::parse(".push-tag").unwrap();
    let user_selector = Selector::parse(".push-userid").unwrap();
    let content_selector = Selector::parse(".push-content").unwrap();

    let title = document
        .select(&title_selector)
        .nth(2)
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            document
                .select(&Selector::parse("title").unwrap())
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| "未知標題".to_string())
        });

    let board = document
        .select(&board_selector)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "Unknown".to_string());

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

        if user.is_empty() || content_raw.is_empty() {
            continue;
        }

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

        let type_match =
            filter_types.is_empty() || filter_types.contains(&comment_type.to_string());

        let keyword_match = keywords
            .as_ref()
            .map_or(true, |k_vec| k_vec.iter().any(|k| content.contains(k)));

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

static TOTAL_COMMENTS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r", 共(\d+)則").unwrap());

pub async fn scrape_ptt_web(user_id: &str, target_boards: &[String]) -> Result<PttWebData, Error> {
    // (新增) 在每次請求前加入 250 毫秒的延遲
    sleep(Duration::from_millis(250)).await;

    let url = format!("https://www.pttweb.cc/user/{}?t=message", user_id);

    let response = reqwest::get(&url).await?;
    let html = response.text().await?;
    let document = Html::parse_document(&html);

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

    let headline_selector = Selector::parse("div.headline").unwrap();
    let headline_text = document
        .select(&headline_selector)
        .next()
        .map(|el| el.text().collect::<String>());

    let total_comments = headline_text
        .as_ref()
        .and_then(|text| TOTAL_COMMENTS_RE.captures(text))
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .ok_or_else(|| {
            Error::PttWebParse(format!("無法從 headline 解析 {} 的總留言數", user_id))
        })?;

    let mut board_comments = HashMap::new();
    let board_item_selector = Selector::parse(".e7-wrapper-board .e7-box").unwrap();
    let board_name_selector = Selector::parse("a").unwrap();
    let board_count_selector = Selector::parse("span.ml-2").unwrap();

    for element in document.select(&board_item_selector) {
        if let Some(name_el) = element.select(&board_name_selector).next() {
            let board_name = name_el.text().collect::<String>().trim().to_string();

            if target_boards.contains(&board_name) {
                if let Some(count_el) = element.select(&board_count_selector).next() {
                    let count_str = count_el.text().collect::<String>();
                    if let Ok(count) = count_str.trim().parse::<u32>() {
                        board_comments.insert(board_name, count);
                    }
                }
            }
        }
    }

    Ok(PttWebData {
        board_comments,
        total_comments,
    })
}
