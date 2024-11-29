use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest::{Client, Url};
use reqwest_middleware::ClientBuilder;
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Highlight {
    id: String,
    parent_id: String,
    content: String,
}

impl Highlight {
    fn new(value: &serde_json::Value) -> Option<Self> {
        Some(Self {
            id: value.get("id")?.as_str()?.trim_matches('"').to_string(),
            parent_id: value
                .get("parent_id")?
                .as_str()?
                .trim_matches('"')
                .to_string(),
            content: value
                .get("content")?
                .as_str()?
                .trim_matches('"')
                .to_string(),
        })
    }
}

struct Article {
    id: String,
    source_url: String,
    title: String,
    author: String,
    saved_at: String,
}

impl Article {
    fn new(value: &serde_json::Value) -> Option<Self> {
        Some(Self {
            id: value.get("id")?.as_str()?.trim_matches('"').to_string(),
            source_url: value
                .get("source_url")?
                .as_str()?
                .trim_matches('"')
                .to_string(),
            title: value.get("title")?.as_str()?.trim_matches('"').to_string(),
            author: value.get("author")?.as_str()?.trim_matches('"').to_string(),
            saved_at: value
                .get("saved_at")?
                .as_str()?
                .trim_matches('"')
                .to_string(),
        })
    }
}

struct Note {
    id: String,
    parent_id: String,
    saved_at: String,
    content: String,
}

impl Note {
    fn new(value: &serde_json::Value) -> Option<Self> {
        Some(Self {
            id: value.get("id")?.as_str()?.trim_matches('"').to_string(),
            parent_id: value
                .get("parent_id")?
                .as_str()?
                .trim_matches('"')
                .to_string(),
            saved_at: value
                .get("saved_at")?
                .as_str()?
                .trim_matches('"')
                .to_string(),
            content: value
                .get("content")?
                .as_str()?
                .trim_matches('"')
                .to_string(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start_time = std::time::Instant::now();
    let target = "playground";
    match target {
        "playground" => playground().await,
        "org" => {
            // Connect to SQLite database
            let home_dir = std::env::var("HOME").expect("HOME environment variable not set");
            let db_path = format!("{}/org-roam/org-roam.db", home_dir);
            let conn = rusqlite::Connection::open(db_path)?;

            // Get all existing refs from database
            let mut stmt = conn.prepare("SELECT ref, node_id FROM refs")?;
            let existing_refs: std::collections::HashMap<String, String> = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .filter_map(Result::ok)
                .map(|(url, node_id): (String, String)| {
                    (url.trim_matches('"').to_string(), node_id)
                })
                .collect();

            // Similarly, get all existing nodes, creating a mapping from id to file and title
            let mut stmt = conn.prepare("SELECT id, file, title FROM nodes")?;
            let existing_nodes: std::collections::HashMap<String, (String, String)> = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .filter_map(Result::ok)
                .map(|(id, file, title): (String, String, String)| {
                    (
                        id,
                        (
                            file.trim_matches('"').to_string(),
                            title.trim_matches('"').to_string(),
                        ),
                    )
                })
                .collect();

            let articles = get_article_list().await?;
            let highlights = get_highlight_list().await?;
            println!("Total articles found: {}", &articles.len());
            println!("Total highlights found: {}", &highlights.len());

            let mut found_parents = 0;
            for highlight in &highlights {
                let parent_id = highlight.parent_id.clone();
                let parent_article = articles.iter().find(|article| article.id == parent_id);

                println!("ID: {}", highlight.id);
                println!("Parent ID: {}", parent_id);
                if let Some(article) = parent_article {
                    found_parents += 1;
                    println!("found the parent: {}", article.source_url);
                }
                println!("Content: {}", highlight.content);
                println!();
            }
            println!(
                "Found parents for {}/{} highlights",
                found_parents,
                highlights.len()
            );

            // Group highlights by parent_id
            let mut highlights_by_parent: HashMap<String, Vec<&Highlight>> = HashMap::new();
            for highlight in &highlights {
                let parent_id = highlight.parent_id.clone();
                highlights_by_parent
                    .entry(parent_id)
                    .or_default()
                    .push(highlight);
            }

            // Process each parent article that has highlights
            for (parent_id, parent_highlights) in &highlights_by_parent {
                // Find the parent article
                if let Some(parent) = articles.iter().find(|a| a.id == *parent_id) {
                    println!("\nHighlights for article {}:", parent_id);
                    for highlight in parent_highlights {
                        println!("- {}", highlight.content);
                    }

                    if let Ok(parsed_url) = Url::parse(&parent.source_url) {
                        let clean_url = format!(
                            "//{}{}",
                            parsed_url.host_str().unwrap_or(""),
                            parsed_url.path()
                        );

                        let filename = get_new_entry_filename(&parent.title);

                        if let Some((existing_file, _)) = existing_refs
                            .get(&clean_url)
                            .and_then(|id| existing_nodes.get(id))
                        {
                            println!("Parent article is in file: {}", existing_file);
                        } else {
                            println!("Parent article would be created in: {}", filename);
                        }
                    }
                }
            }
            let mut articles_processed = 0;
            for (parent_id, parent_highlights) in highlights_by_parent.iter() {
                // Find the parent article
                if let Some(parent) = articles.iter().find(|a| a.id == *parent_id) {
                    if let Ok(parsed_url) = Url::parse(&parent.source_url) {
                        let clean_url = format!(
                            "{}{}",
                            parsed_url.host_str().unwrap_or(""),
                            parsed_url.path()
                        );
                        let roam_db_url = format!("//{}", clean_url);
                        let full_url = format!("{}://{}", parsed_url.scheme(), clean_url);

                        let filename = get_new_entry_filename(&parent.title);

                        let uuid = uuid::Uuid::new_v4().to_string();
                        // Skip if file already exists in org-roam
                        if existing_refs
                            .get(&roam_db_url)
                            .and_then(|id| existing_nodes.get(id))
                            .is_none()
                        {
                            // Create file and write highlights
                            let mut content = String::new();
                            content.push_str(":PROPERTIES:\n");
                            content.push_str(&format!(":ID: {}\n", uuid));
                            content.push_str(&format!(":ROAM_REFS: {}\n", full_url));
                            content.push_str(":END:\n");
                            content.push_str(&format!("#+TITLE: {}\n", parent.title));
                            content.push_str(&format!("#+roam_key: {}\n", full_url));
                            content.push_str("\n* Highlights\n");

                            for highlight in parent_highlights {
                                if !highlight.content.is_empty() {
                                    content.push_str(&format!("- {}\n", highlight.content));
                                }
                            }

                            std::fs::write(&filename, &content)?;
                            println!("Created file: {}", filename);
                            println!("Content:\n{}", content);
                        }
                    }
                }
                articles_processed += 1;
            }
            println!("\nProcessed {} articles", articles_processed);
            let duration = start_time.elapsed();
            println!("Time taken: {:?}", duration);
            Ok(())
        }
        _ => panic!("invalid target {}", target),
    }
}

async fn playground() -> Result<(), Box<dyn std::error::Error>> {
    let target = "notes";

    if target == "notes" {
        let notes = get_note_list().await?;
        for note in notes {
            println!("ID: {}", note.id);
            println!("Parent ID: {}", note.parent_id);
            println!("Saved at: {}", note.saved_at);
            println!("Content: {}", note.content);
            println!(); // Empty line between entries
        }
    } else if target == "articles" {
        let articles = get_article_list().await?;
        for article in articles {
            println!("ID: {}", article.id);
            println!("URL: {}", article.source_url);
            println!("Author: {}", article.author);
            println!("Saved at: {}", article.saved_at);
            println!("Title: {}", article.title);
            println!(); // Empty line between entries
        }
    } else if target == "highlights" {
        let highlights = get_highlight_list().await?;
        for highlight in highlights {
            let content = highlight.content;

            let id = highlight.id;

            let parent_id = highlight.parent_id;

            println!("Content: {}", content);
            println!("ID: {}", id);
            println!("Parent ID: {}", parent_id);
            println!(); // Empty line between entries
        }
    }
    Ok(())
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

        println!("Request URL: {}", url);
        println!("Cache Status: {:?}", response.headers().get("x-cache"));

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

async fn get_article_list() -> Result<Vec<Article>, Box<dyn std::error::Error>> {
    let json_results = fetch_readwise_data(Some("article")).await?;
    let articles = json_results
        .into_iter()
        .filter_map(|value| Article::new(&value))
        .collect();
    Ok(articles)
}

async fn get_note_list() -> Result<Vec<Note>, Box<dyn std::error::Error>> {
    let json_results = fetch_readwise_data(Some("note")).await?;
    let notes = json_results
        .into_iter()
        .filter_map(|value| Note::new(&value))
        .collect();
    Ok(notes)
}

async fn get_highlight_list() -> Result<Vec<Highlight>, Box<dyn std::error::Error>> {
    let json_results = fetch_readwise_data(Some("highlight")).await?;
    let highlights = json_results
        .into_iter()
        .filter_map(|value| Highlight::new(&value))
        .collect();
    Ok(highlights)
}

fn get_new_entry_filename(title: &str) -> String {
    let now = chrono::Local::now();
    let home_dir = std::env::var("HOME").expect("HOME environment variable not set");
    format!(
        "{}/org/roam/{}-{}.org",
        home_dir,
        now.format("%Y%m%d%H%M%S"),
        slug::slugify(title)
    )
}
