use reqwest::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let debug = true;
    let target = "highlights";

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
    let client = reqwest::Client::new();
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

        let data: serde_json::Value = response.json().await?;

        if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
            all_results.extend(results.clone());
        }

        if debug {
            break;
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
    fetch_readwise_data(debug, None).await
}

async fn get_note_list(debug: bool) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    fetch_readwise_data(debug, Some("note")).await
}

async fn get_highlight_list(
    debug: bool,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    fetch_readwise_data(debug, Some("highlight")).await
}
