use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest::{Client, Url};
use reqwest_middleware::ClientBuilder;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let debug = true;
    let target = "org";
    match target {
        "playground" => playground(debug).await,
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
            let mut existing_nodes: std::collections::HashMap<String, (String, String)> = stmt
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

            let articles = get_reader_list(debug).await?;
            let highlights = get_highlight_list(debug).await?;
            println!("Total articles found: {}", &articles.len());
            println!("Total highlights found: {}", &highlights.len());

            let mut found_parents = 0;
            for item in &highlights {
                let parent_id = item.get("parent_id").unwrap().as_str().unwrap();
                let parent_url = articles
                    .iter()
                    .find(|article| article["id"].as_str() == Some(parent_id))
                    .and_then(|article| article["source_url"].as_str())
                    .map(String::from);

                println!(
                    "ID: {}",
                    item.get("id").unwrap().as_str().unwrap().trim_matches('"')
                );
                println!("Parent ID: {}", parent_id);
                if parent_url.is_some() {
                    found_parents += 1;
                    println!("found the parent: {}", parent_url.unwrap());
                }
                println!(
                    "Content: {}",
                    item.get("content")
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .trim_matches('"')
                );
                println!();
            }
            println!(
                "Found parents for {}/{} highlights",
                found_parents,
                highlights.len()
            );

            // Group highlights by parent_id
            let mut highlights_by_parent: HashMap<String, Vec<&serde_json::Value>> = HashMap::new();
            for highlight in &highlights {
                let parent_id = highlight
                    .get("parent_id")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string();
                highlights_by_parent
                    .entry(parent_id)
                    .or_default()
                    .push(highlight);
            }

            // Process each parent article that has highlights
            for (parent_id, parent_highlights) in &highlights_by_parent {
                // Find the parent article
                if let Some(parent) = articles
                    .iter()
                    .find(|a| a["id"].as_str() == Some(parent_id))
                {
                    println!("\nHighlights for article {}:", parent_id);
                    for highlight in parent_highlights {
                        println!(
                            "- {}",
                            highlight
                                .get("id")
                                .unwrap()
                                .as_str()
                                .unwrap()
                                .trim_matches('"')
                        );
                    }

                    if let Some(url) = parent.get("source_url").and_then(|u| u.as_str()) {
                        if let Ok(parsed_url) = Url::parse(url) {
                            let clean_url = format!(
                                "//{}{}",
                                parsed_url.host_str().unwrap_or(""),
                                parsed_url.path()
                            );

                            let title = parent
                                .get("title")
                                .and_then(|t| t.as_str())
                                .unwrap_or("No title");

                            let filename = get_new_entry_filename(title);

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
            }
            let mut articles_processed = 0;
            for (parent_id, parent_highlights) in highlights_by_parent.iter() {
                // Find the parent article
                if let Some(parent) = articles
                    .iter()
                    .find(|a| a["id"].as_str() == Some(parent_id))
                {
                    if let Some(url) = parent.get("source_url").and_then(|u| u.as_str()) {
                        if let Ok(parsed_url) = Url::parse(url) {
                            let clean_url = format!(
                                "{}{}",
                                parsed_url.host_str().unwrap_or(""),
                                parsed_url.path()
                            );
                            let roam_db_url = format!("//{}", clean_url);
                            let full_url = format!("{}://{}", parsed_url.scheme(), clean_url);

                            let title = parent
                                .get("title")
                                .and_then(|t| t.as_str())
                                .unwrap_or("No title");

                            let filename = get_new_entry_filename(title);

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
                                content.push_str(&format!("#+TITLE: {}\n", title));
                                content.push_str(&format!("#+roam_key: {}\n", full_url));
                                content.push_str("\n* Highlights\n");

                                for highlight in parent_highlights {
                                    if let Some(text) =
                                        highlight.get("content").and_then(|t| t.as_str())
                                    {
                                        if !text.is_empty() {
                                            content.push_str(&format!("- {}\n", text));
                                        }
                                    }
                                }

                                std::fs::write(&filename, &content)?;
                                println!("Created file: {}", filename);
                                println!("Content:\n{}", content);
                            }
                        }
                    }
                }
                articles_processed += 1;
            }
            println!("\nProcessed {} articles", articles_processed);
            Ok(())
        }
        _ => panic!("invalid target {}", target),
    }
}

async fn playground(debug: bool) -> Result<(), Box<dyn std::error::Error>> {
    let target = "articles";

    if target == "notes" {
        let notes = get_note_list(debug).await?;
        for note in notes {
            let id = note
                .get("id")
                .and_then(|i| i.as_str())
                .expect("Note must have an id");

            let parent_id = note
                .get("parent_id")
                .and_then(|p| p.as_str())
                .expect("Note must have a parent_id");

            let saved_at = note
                .get("saved_at")
                .and_then(|s| s.as_str())
                .expect("Note must have saved_at");

            let content = note
                .get("content")
                .and_then(|c| c.as_str())
                .expect("Note must have content");

            println!("ID: {}", id);
            println!("Parent ID: {}", parent_id);
            println!("Saved at: {}", saved_at);
            println!("Content: {}", content);
            println!(); // Empty line between entries
        }
    } else if target == "articles" {
        let results = get_reader_list(debug).await?;
        for item in results {
            if let Some(category) = item.get("category").and_then(|c| c.as_str()) {
                if category == "article" {
                    if let Some(url) = item.get("source_url").and_then(|u| u.as_str()) {
                        if let Ok(parsed_url) = Url::parse(url) {
                            let clean_url = format!(
                                "{}://{}{}",
                                parsed_url.scheme(),
                                parsed_url.host_str().unwrap_or(""),
                                parsed_url.path()
                            );

                            let id = item
                                .get("id")
                                .and_then(|i| i.as_str())
                                .expect("Article must have an id");

                            let author = item
                                .get("author")
                                .and_then(|a| a.as_str())
                                .expect("Article must have an author");

                            let saved_at = item
                                .get("saved_at")
                                .and_then(|s| s.as_str())
                                .expect("Article must have saved_at");

                            let title = item
                                .get("title")
                                .and_then(|t| t.as_str())
                                .expect("Article must have a title");

                            println!("ID: {}", id);
                            println!("URL: {}", clean_url);
                            println!("Author: {}", author);
                            println!("Saved at: {}", saved_at);
                            println!("Title: {}", title);
                            println!(); // Empty line between entries
                        }
                    }
                }
            }
        }
    } else if target == "highlights" {
        let highlights = get_highlight_list(debug).await?;
        for highlight in highlights {
            let content = highlight
                .get("content")
                .and_then(|c| c.as_str())
                .expect("Highlight must have content");

            let created_at = highlight
                .get("created_at")
                .and_then(|c| c.as_str())
                .expect("Highlight must have created_at");

            let id = highlight
                .get("id")
                .and_then(|i| i.as_str())
                .expect("Highlight must have id");

            let parent_id = highlight
                .get("parent_id")
                .and_then(|p| p.as_str())
                .expect("Highlight must have parent_id");

            println!("Content: {}", content);
            println!("Created at: {}", created_at);
            println!("ID: {}", id);
            println!("Parent ID: {}", parent_id);
            println!(); // Empty line between entries
        }
    }
    Ok(())
}

async fn fetch_readwise_data(
    debug: bool,
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

async fn get_reader_list(
    debug: bool,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    fetch_readwise_data(debug, Some("article")).await
}

async fn get_note_list(debug: bool) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    fetch_readwise_data(debug, Some("note")).await
}

async fn get_highlight_list(
    debug: bool,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    fetch_readwise_data(debug, Some("highlight")).await
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
