use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest::{Client, Url};
use reqwest_middleware::ClientBuilder;

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

            // Get articles and check if they exist in database
            let results = get_reader_list(debug).await?;
            println!("Total articles found: {}", &results.len());
            for item in results {
                if let Some(url) = item.get("source_url").and_then(|u| u.as_str()) {
                    if let Ok(parsed_url) = Url::parse(url) {
                        let clean_url = format!(
                            "//{}{}",
                            parsed_url.host_str().unwrap_or(""),
                            parsed_url.path()
                        );

                        let title = item
                            .get("title")
                            .and_then(|t| t.as_str())
                            .unwrap_or("No title");

                        let now = chrono::Local::now();
                        let filename = format!(
                            "{}/org-roam/{}-{}.org",
                            home_dir,
                            now.format("%Y%m%d%H%M%S"),
                            slug::slugify(title)
                        );

                        if existing_refs.contains_key(&clean_url) {
                            println!(
                                "Ref already exists: {}, in node_id {}",
                                clean_url,
                                existing_refs.get(&clean_url).unwrap()
                            );
                            // Print the corresponding filename
                            let node_id = existing_refs.get(&clean_url).unwrap();
                            let (file, title) = existing_nodes.get(node_id).unwrap();
                            println!("Corresponding node: {} - {}", file, title);

                            let file_contents = std::fs::read_to_string(file)?;
                            println!("Contents of corresponding node: {}", file_contents);
                        } else {
                            // println!("would create: {} with ref {}", filename, clean_url);
                        }
                    }
                }
            }
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
                "{}{}",
                if category.is_some() { "&" } else { "?" },
                format!("pageCursor={}", cursor)
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
