mod readwise_api;
use readwise_api::*;
use reqwest::Url;
use std::collections::HashMap;
use tera::{Context, Tera};

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
    // Connect to SQLite database
    let home_dir = std::env::var("HOME").expect("HOME environment variable not set");
    let tera = Tera::new("templates/**/*").unwrap();
    let db_path = format!("{}/org-roam/org-roam.db", home_dir);
    let conn = rusqlite::Connection::open(db_path)?;

    let existing_refs = get_refs_from_db(&conn)?;
    let existing_nodes = get_nodes_from_db(&conn)?;

    let articles = get_article_list().await?;
    let highlights = get_highlight_list().await?;
    let notes = get_note_list().await?;
    println!("Total articles found: {}", &articles.len());
    println!("Total highlights found: {}", &highlights.len());
    println!("Total notes found: {}", &notes.len());
    for location in ["new", "later", "shortlist", "archive", "feed"] {
        let filtered_articles: Vec<&Article> =
            articles.iter().filter(|a| a.location == location).collect();
        println!("Articles in {}: {}", location, filtered_articles.len());
        if !filtered_articles.is_empty() {
            println!("First article: {:?}", filtered_articles[0]);
        }
    }
    println!("First highlight: {:?}", highlights[0]);
    println!("First note: {:?}", notes[0]);
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

    let mut found_highlight_parents = 0;
    for highlight in &highlights {
        let parent_id = highlight.parent_id.clone();
        let parent_article = articles.iter().find(|article| article.id == parent_id);

        if parent_article.is_some() {
            found_highlight_parents += 1;
        }
    }
    println!(
        "Found parent articles for {}/{} highlights",
        found_highlight_parents,
        highlights.len()
    );

    let mut found_note_parents = 0;
    for note in &notes {
        let parent_id = note.parent_id.clone();
        let parent_highlight = highlights
            .iter()
            .find(|highlight| highlight.id == parent_id);

        if parent_highlight.is_some() {
            found_note_parents += 1;
        }
    }
    println!(
        "Found parent highlights for {}/{} notes",
        found_note_parents,
        notes.len()
    );

    let highlights_by_parent = map_parents_to_highlights(articles.clone(), highlights);
    println!("{} parents", highlights_by_parent.len());
    let notes_by_parent = note_list_to_map(notes);

    let mut articles_processed = 0;

    for parent_id in highlights_by_parent.keys().cloned() {
        // Find the parent article
        if let Some(parent) = articles.iter().find(|a| a.id == parent_id) {
            if let Ok(parsed_url) = Url::parse(&parent.source_url) {
                let clean_url = format!(
                    "{}{}",
                    parsed_url.host_str().unwrap_or(""),
                    parsed_url.path()
                );
                let full_url = format!("{}://{}", parsed_url.scheme(), clean_url);
                // org-roam stores URLs as UTF-8, not as percent-encoded
                let roam_db_url = urlencoding::decode(&format!("//{}", clean_url))
                    .expect("UTF-8")
                    .to_string();

                let filename = get_new_entry_filename(&parent.title);

                let uuid = uuid::Uuid::new_v4().to_string();
                // Skip if file already exists in org-roam
                if existing_refs
                    .get(&roam_db_url)
                    .and_then(|id| existing_nodes.get(id))
                    .is_some()
                {
                    continue;
                }

                let mut context = Context::new();
                context.insert("uuid", &uuid);
                context.insert("full_url", &full_url);
                context.insert("title", &parent.title);
                context.insert(
                    "today",
                    &chrono::Local::now().format("%Y-%m-%d %a").to_string(),
                );
                context.insert(
                    "read_status",
                    match parent.location.as_str() {
                        "new" => "TODO",
                        "later" => "TODO",
                        "shortlist" => "TODO",
                        "archive" => "DONE",
                        _ => "TODO",
                    },
                );

                if let Some(entry_highlights) = highlights_by_parent.get(&parent_id) {
                    // Create a vector of highlights with their notes
                    let highlights_with_notes: Vec<_> = entry_highlights
                        .iter()
                        .map(|highlight| {
                            let note = notes_by_parent.get(&highlight.id);
                            serde_json::json!({
                                "id": highlight.id,
                                "content": highlight.content,
                                "note": note.map(|n| n.content.clone()),
                            })
                        })
                        .collect();
                    context.insert("highlights", &highlights_with_notes);
                }
                let content = tera.render("article.org.tera", &context)?;
                std::fs::write(&filename, &content)?;
                println!("Created file: {}", filename);
            }
        }
        articles_processed += 1;
    }
    println!("\nProcessed {} articles", articles_processed);
    let duration = start_time.elapsed();
    println!("Time taken: {:?}", duration);
    Ok(())
}

fn get_new_entry_filename(title: &str) -> String {
    let now = chrono::Local::now();
    let home_dir = std::env::var("HOME").expect("HOME environment variable not set");
    let slug = slug::slugify(title);
    let truncated_slug = if slug.len() > 100 {
        slug[..100].to_string()
    } else {
        slug
    };
    format!(
        "{}/org/roam/{}-{}.org",
        home_dir,
        now.format("%Y%m%d%H%M%S"),
        truncated_slug
    )
}
