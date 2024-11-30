use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Highlight {
    pub id: String,
    pub parent_id: String,
    pub content: String,
}

impl Highlight {
    fn new(value: &serde_json::Value) -> Option<Self> {
        Some(Self {
            id: value.get("id")?.as_str()?.to_string(),
            parent_id: value.get("parent_id")?.as_str()?.to_string(),
            content: value.get("content")?.as_str()?.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Article {
    pub id: String,
    pub source_url: String,
    pub title: String,
    pub category: String,
    pub location: String,
    pub author: String,
    pub saved_at: String,
}

impl Article {
    fn new(value: &serde_json::Value) -> Option<Self> {
        Some(Self {
            id: value.get("id")?.as_str()?.to_string(),
            source_url: value.get("source_url")?.as_str()?.to_string(),
            title: value.get("title")?.as_str()?.to_string(),
            category: value.get("category")?.as_str()?.to_string(),
            location: value.get("location")?.as_str()?.to_string(),
            author: value.get("author")?.as_str()?.to_string(),
            saved_at: value.get("saved_at")?.as_str()?.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Note {
    pub id: String,
    pub parent_id: String,
    pub saved_at: String,
    pub content: String,
}

impl Note {
    fn new(value: &serde_json::Value) -> Option<Self> {
        Some(Self {
            id: value.get("id")?.as_str()?.to_string(),
            parent_id: value.get("parent_id")?.as_str()?.to_string(),
            saved_at: value.get("saved_at")?.as_str()?.to_string(),
            content: value.get("content")?.as_str()?.to_string(),
        })
    }
}
async fn fetch_readwise_data(
    category: Option<&str>,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    let api_key = std::env::var("READWISE_API_KEY")?;

    let client = ClientBuilder::new(Client::new())
        .with(Cache(HttpCache {
            mode: CacheMode::IgnoreRules,
            manager: CACacheManager::default(),
            options: HttpCacheOptions::default(),
        }))
        .build();

    let mut all_results = Vec::new();
    let mut next_cursor = None;

    loop {
        let mut url = String::from("https://readwise.io/api/v3/list/");
        if let Some(cat) = category {
            url.push_str(&format!("?category={}", cat));
        }

        if let Some(cursor) = next_cursor {
            url.push_str(&format!(
                "{}pageCursor={}",
                if category.is_some() { "&" } else { "?" },
                cursor
            ));
        }

        let response = client
            .get(&url)
            .header("Authorization", format!("Token {}", api_key))
            .send()
            .await?;

        println!(
            "Cache {:?} {}",
            response.headers().get("x-cache").unwrap(),
            url
        );

        let data: serde_json::Value = response.json().await?;

        if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
            all_results.extend(results.clone());
        }

        next_cursor = data
            .get("nextPageCursor")
            .and_then(|c| c.as_str())
            .map(String::from);

        if next_cursor.is_none() {
            break;
        }
    }

    Ok(all_results)
}

pub async fn get_article_list() -> Result<Vec<Article>, Box<dyn std::error::Error>> {
    let json_results = fetch_readwise_data(Some("article")).await?;
    let articles = json_results
        .into_iter()
        .filter_map(|value| Article::new(&value))
        .collect();
    Ok(articles)
}

pub async fn get_note_list() -> Result<Vec<Note>, Box<dyn std::error::Error>> {
    let json_results = fetch_readwise_data(Some("note")).await?;
    let notes = json_results
        .into_iter()
        .filter_map(|value| Note::new(&value))
        .collect();
    Ok(notes)
}

pub async fn get_highlight_list() -> Result<Vec<Highlight>, Box<dyn std::error::Error>> {
    let json_results = fetch_readwise_data(Some("highlight")).await?;
    let highlights: Vec<Highlight> = json_results
        .into_iter()
        .filter_map(|value| Highlight::new(&value))
        // There's a surprising number of empty highlights
        .filter(|h| !h.content.is_empty())
        .collect();
    Ok(highlights)
}

pub fn highlight_list_to_map(highlight_list: Vec<Highlight>) -> HashMap<String, Vec<Highlight>> {
    // Return a map of parent_id to a list of the corresponding highlights
    let mut map: HashMap<String, Vec<Highlight>> = HashMap::new();
    for highlight in highlight_list {
        map.entry(highlight.parent_id.clone())
            .or_default()
            .push(highlight);
    }
    map
}

pub fn note_list_to_map(note_list: Vec<Note>) -> HashMap<String, Note> {
    // Return a map of parent_id (a highlight id) to the corresponding note
    note_list
        .into_iter()
        .map(|note| (note.parent_id.clone(), note))
        .collect()
}
