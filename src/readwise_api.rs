use crate::{org_roam::org_roam_ref, util::clean_url};
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
pub struct Document {
    pub id: String,
    // roam_db_ref and roam_full_ref are similar but with a few key differences:
    // - for documents with an URL, the full ref is e.g. https://example.com, while the db ref is only //example.com
    // - for documents without an URL, we use their ID to create a ref that must start with an @ symbol for the full ref,
    //   while the db ref is the same but without the leading @ symbol.
    pub roam_db_ref: String,
    pub roam_full_ref: String,
    pub source_url: String,
    pub title: String,
    pub category: String,
    pub location: String,
    pub author: String,
    pub saved_at: String,
}

impl Document {
    fn new(value: &serde_json::Value) -> Option<Self> {
        let clean_url = clean_url(value.get("source_url")?.as_str()?);
        let id = value.get("id")?.as_str()?.to_string();
        let category = value.get("category")?.as_str()?.to_string();
        Some(Self {
            id: id.clone(),
            roam_db_ref: match category.as_str() {
                "article" => org_roam_ref(&clean_url),
                "epub" => format!("readwise_{}", id),
                _ => panic!("Unknown category type: {}", category),
            },
            roam_full_ref: match category.as_str() {
                "article" => clean_url.clone(),
                "epub" => format!("@readwise_{}", id),
                _ => panic!("Unknown category type: {}", category),
            },
            source_url: clean_url,
            title: value.get("title")?.as_str()?.to_string(),
            category,
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
        let mut params = Vec::new();

        if let Some(cat) = category {
            params.push(format!("category={}", cat));
        }

        if let Some(cursor) = next_cursor {
            params.push(format!("pageCursor={}", cursor));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
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

pub async fn get_document_list() -> Result<Vec<Document>, Box<dyn std::error::Error>> {
    // Return all documents of type "epub" or "article"
    let mut all_documents = Vec::new();

    for category in ["epub", "article"] {
        let results = fetch_readwise_data(Some(category)).await?;
        let documents: Vec<Document> = results
            .into_iter()
            .filter_map(|value| Document::new(&value))
            .collect();
        all_documents.extend(documents);
    }

    Ok(all_documents)
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

pub fn map_parents_to_highlights(
    articles: Vec<Document>,
    highlights: Vec<Highlight>,
) -> HashMap<String, Vec<Highlight>> {
    // Create a map from parent article IDs to their highlights
    let mut parent_map: HashMap<String, Vec<Highlight>> = HashMap::new();

    // Initialize empty vectors for each article ID
    for article in articles {
        parent_map.insert(article.id, Vec::new());
    }

    // Group highlights by their parent_id
    for highlight in highlights {
        if let Some(highlights_vec) = parent_map.get_mut(&highlight.parent_id) {
            highlights_vec.push(highlight);
        }
    }

    parent_map
}

pub fn note_list_to_map(note_list: Vec<Note>) -> HashMap<String, Note> {
    // Return a map of parent_id (a highlight id) to the corresponding note
    note_list
        .into_iter()
        .map(|note| (note.parent_id.clone(), note))
        .collect()
}
