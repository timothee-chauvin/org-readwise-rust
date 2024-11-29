mod readwise_api;
use readwise_api::*;
use reqwest::Url;
use std::collections::HashMap;

fn get_refs_from_db(
    conn: &rusqlite::Connection,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    // Return a mapping from ref (the special URL format that org-roam uses) to node_id (the ID of the node where this ref is found)
    let mut stmt = conn.prepare("SELECT ref, node_id FROM refs")?;
    let existing_refs: HashMap<String, String> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(Result::ok)
        .map(|(url, node_id): (String, String)| {
            (
                url.trim_matches('"').to_string(),
                node_id.trim_matches('"').to_string(),
            )
        })
        .collect();
    Ok(existing_refs)
}

fn get_nodes_from_db(
    conn: &rusqlite::Connection,
) -> Result<HashMap<String, (String, String)>, Box<dyn std::error::Error>> {
    // Return a mapping from the ID of the node to the file where it is found and its title
    let mut stmt = conn.prepare("SELECT id, file, title FROM nodes")?;
    let nodes: HashMap<String, (String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, (row.get(1)?, row.get(2)?))))?
        .filter_map(Result::ok)
        .map(|(id, (file, title)): (String, (String, String))| {
            (
                id.trim_matches('"').to_string(),
                (
                    file.trim_matches('"').to_string(),
                    title.trim_matches('"').to_string(),
                ),
            )
        })
        .collect();
    Ok(nodes)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start_time = std::time::Instant::now();
    let target = "org";
    match target {
        "playground" => playground().await,
        "org" => {
            // Connect to SQLite database
            let home_dir = std::env::var("HOME").expect("HOME environment variable not set");
            let db_path = format!("{}/org-roam/org-roam.db", home_dir);
            let conn = rusqlite::Connection::open(db_path)?;

            let existing_refs = get_refs_from_db(&conn)?;
            let existing_nodes = get_nodes_from_db(&conn)?;

            let articles = get_article_list().await?;
            let highlights = get_highlight_list().await?;
            println!("Total articles found: {}", &articles.len());
            println!("Total highlights found: {}", &highlights.len());

            println!("First article: {:?}", articles[0]);
            println!("First highlight: {:?}", highlights[0]);
            let first_ref = existing_refs.keys().next().unwrap();
            println!(
                "First ref: {:?} => {:?}",
                first_ref,
                existing_refs.get(first_ref).unwrap()
            );
            let first_node = existing_nodes.keys().next().unwrap();
            println!(
                "First node: {:?} => {:?}",
                first_node,
                existing_nodes.get(first_node).unwrap()
            );

            let mut found_parents = 0;
            for highlight in &highlights {
                let parent_id = highlight.parent_id.clone();
                let parent_article = articles.iter().find(|article| article.id == parent_id);

                if parent_article.is_some() {
                    found_parents += 1;
                }
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
                                content.push_str(&format!("- {}\n", highlight.content));
                            }

                            std::fs::write(&filename, &content)?;
                            println!("Created file: {}", filename);
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
