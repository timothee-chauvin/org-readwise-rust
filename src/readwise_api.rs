use crate::util::clean_url;
use crate::SETTINGS;

use chrono::Utc;
use reqwest::{Client, StatusCode};
use std::collections::HashMap;
use std::fs;
use tokio::time::{sleep, Duration};

#[derive(Debug, Clone)]
pub struct Highlight {
    pub id: String,
    pub parent_id: String,
    pub content: String,
}

fn get_string(
    value: &serde_json::Value,
    field: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(value
        .get(field)
        .ok_or(format!("Missing {}", field))?
        .as_str()
        .ok_or(format!("{} is not a string", field))?
        .to_string())
}

impl Highlight {
    fn new(value: &serde_json::Value) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            id: get_string(value, "id")?,
            parent_id: get_string(value, "parent_id")?,
            content: get_string(value, "content")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    pub id: String,
    // A document has a URL if the "source_url" field in the API results starts with http
    // (typically, otherwise the source_url starts with private://)
    pub has_url: bool,
    // roam_ref is either the full URL if there is one, or a ref in the format @readwise_<id>
    pub roam_ref: String,
    pub source_url: String,
    pub readwise_url: String,
    pub title: String,
    pub location: String,
    pub author: String,
    pub saved_at: chrono::DateTime<Utc>,
    pub published_date: Option<chrono::DateTime<Utc>>,
}

impl Document {
    fn new(value: &serde_json::Value) -> Result<Self, Box<dyn std::error::Error>> {
        let source_url = get_string(value, "source_url")?;
        let has_url = source_url.starts_with("http");
        let clean_url = clean_url(&source_url);
        let id = get_string(value, "id")?;
        // published_date is either Null, or a Unix timestamp like Number(1064880000000)
        let published_date = match value.get("published_date").and_then(|v| v.as_i64()) {
            Some(timestamp) => chrono::DateTime::from_timestamp(timestamp / 1000, 0),
            None => None,
        };
        let category = get_string(value, "category")?;
        // For a book with a published date, edit the title to be "Title (Year)"
        let title = if category == "epub" && published_date.is_some() {
            format!(
                "{} ({})",
                get_string(value, "title")?,
                published_date.unwrap().format("%Y")
            )
        } else {
            get_string(value, "title")?
        };
        Ok(Self {
            id: id.clone(),
            has_url,
            roam_ref: match has_url {
                true => clean_url.clone(),
                false => format!("@readwise_{}", id),
            },
            source_url: clean_url,
            readwise_url: get_string(value, "url")?,
            title,
            location: get_string(value, "location")?,
            author: get_string(value, "author")?,
            // saved_at is an ISO 8601 timestamp
            saved_at: chrono::DateTime::parse_from_rfc3339(&get_string(value, "saved_at")?)
                .unwrap()
                .with_timezone(&Utc),
            published_date,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Note {
    pub parent_id: String,
    pub saved_at: String,
    pub content: String,
}

impl Note {
    fn new(value: &serde_json::Value) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            parent_id: get_string(value, "parent_id")?,
            saved_at: get_string(value, "saved_at")?,
            content: get_string(value, "content")?,
        })
    }
}

async fn fetch_readwise_data(
    category: Option<&str>,
    updated_after: Option<&str>,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    dotenv::from_path(SETTINGS.config_dir.join(".env")).ok();
    let api_key = std::env::var("READWISE_API_KEY")?;

    let client = Client::new();

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

        if let Some(updated_after) = updated_after {
            params.push(format!("updatedAfter={}", updated_after));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        println!("Fetching {}...", url);

        let mut retry_count = 0;
        let max_retries = 5;
        let response = loop {
            let response = client
                .get(&url)
                .header("Authorization", format!("Token {}", api_key))
                .send()
                .await?;

            let status = response.status();
            match status {
                StatusCode::OK => break response,
                StatusCode::TOO_MANY_REQUESTS => {
                    let response_text = response.text().await?;
                    println!("{} - {}", status, response_text);
                    if retry_count >= max_retries {
                        return Err(format!(
                            "Still getting rate limited despite {} retries",
                            retry_count
                        )
                        .into());
                    }
                    // The response text looks like {"detail":"Request was throttled. Expected available in 50 seconds."}
                    // Try to parse the wait time from response
                    let wait_s = if let Some(seconds) = response_text
                        .split("Expected available in ")
                        .nth(1)
                        .and_then(|s| s.split(' ').next())
                        .and_then(|n| n.parse::<u64>().ok())
                    {
                        // Add 5 seconds buffer
                        println!("Waiting {} seconds before retry...", seconds + 5);
                        seconds + 5
                    } else {
                        // Default to 60s if parsing fails
                        println!(
                            "Failed to parse wait time from response. Waiting 60s before retry..."
                        );
                        60
                    };

                    sleep(Duration::from_secs(wait_s)).await;
                    retry_count += 1;
                }
                _ => {
                    return Err(format!(
                        "Unexpected status: {} - {}",
                        status,
                        response.text().await?
                    )
                    .into());
                }
            }
        };

        let data: serde_json::Value = response.json().await?;

        let results = data
            .get("results")
            .and_then(|r| r.as_array())
            .ok_or("No results found in response")?;
        all_results.extend(results.clone());

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

pub async fn get_document_list(
    updated_after: Option<&str>,
) -> Result<Vec<Document>, Box<dyn std::error::Error>> {
    // Return all documents of type "epub" or "article"
    let mut all_documents = Vec::new();

    for category in &SETTINGS.document_categories {
        let results = fetch_readwise_data(Some(category), updated_after).await?;
        println!("Number of {}s: {}", category, results.len());
        let documents: Vec<Document> = results
            .into_iter()
            .filter_map(|value| Document::new(&value).ok())
            .collect();
        all_documents.extend(documents);
    }

    Ok(all_documents)
}

pub async fn get_note_list() -> Result<Vec<Note>, Box<dyn std::error::Error>> {
    let json_results = fetch_readwise_data(Some("note"), None).await?;
    println!("Number of notes: {}", json_results.len());
    let notes = json_results
        .into_iter()
        .filter_map(|value| Note::new(&value).ok())
        .collect();
    Ok(notes)
}

pub async fn get_highlight_list() -> Result<Vec<Highlight>, Box<dyn std::error::Error>> {
    let json_results = fetch_readwise_data(Some("highlight"), None).await?;
    println!("Number of highlights: {}", json_results.len());
    let highlights: Vec<Highlight> = json_results
        .into_iter()
        .filter_map(|value| Highlight::new(&value).ok())
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

pub fn get_updated_after() -> Result<Option<String>, Box<dyn std::error::Error>> {
    // Return the last updated_after date from the updated_after_file_path as a string,
    // or return None if the file doesn't exist.
    // Returns an error if the file exists but contains an invalid date.
    let path = &SETTINGS.updated_after_file_path;
    if !path.exists() {
        return Ok(None);
    }

    // Try to read the existing date from the file and validate it can parse as a date
    let contents = fs::read_to_string(path)?;
    let trimmed = contents.trim();

    match trimmed.parse::<chrono::DateTime<Utc>>() {
        Ok(_) => Ok(Some(trimmed.to_string())),
        Err(e) => Err(format!("Invalid date format in {}: {}", path.display(), e).into()),
    }
}

pub fn save_updated_after(date: &str) {
    fs::write(&SETTINGS.updated_after_file_path, date).unwrap();
}
