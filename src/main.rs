use reqwest::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let debug = true;
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
                            .unwrap_or("Unknown");

                        let saved_at = item
                            .get("saved_at")
                            .and_then(|s| s.as_str())
                            .unwrap_or("Unknown");

                        let title = item
                            .get("title")
                            .and_then(|t| t.as_str())
                            .unwrap_or("Unknown");

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
    Ok(())
}

async fn get_reader_list(
    debug: bool,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    let api_key = std::env::var("READWISE_API_KEY")?;
    let client = reqwest::Client::new();
    let mut all_results = Vec::new();
    let mut next_cursor = None;

    loop {
        let mut url = String::from("https://readwise.io/api/v3/list/");
        if let Some(cursor) = next_cursor {
            url.push_str(&format!("?pageCursor={}", cursor));
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
