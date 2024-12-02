mod org_roam;
mod readwise_api;
mod util;

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

    let documents = get_document_list().await?;
    let highlights = get_highlight_list().await?;
    let notes = get_note_list().await?;
    println!("Total documents found: {}", &documents.len());
    println!("Total highlights found: {}", &highlights.len());
    println!("Total notes found: {}", &notes.len());
    for location in ["new", "later", "shortlist", "archive", "feed"] {
        let filtered_documents: Vec<&Document> = documents
            .iter()
            .filter(|a| a.location == location)
            .collect();
        println!("Documents in {}: {}", location, filtered_documents.len());
        if !filtered_documents.is_empty() {
            println!("First document: {:?}", filtered_documents[0]);
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
        let parent_document = documents.iter().find(|document| document.id == parent_id);

        if parent_document.is_some() {
            found_highlight_parents += 1;
        }
    }
    println!(
        "Found parent documents for {}/{} highlights",
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

    let highlights_by_parent = map_parents_to_highlights(documents.clone(), highlights);
    println!("{} parents", highlights_by_parent.len());
    let notes_by_parent = note_list_to_map(notes);

    let duplicate_titles = get_duplicate_titles(&documents);
    println!("Duplicate titles: {:?}", duplicate_titles);

    let mut documents_processed = 0;

    for parent_id in highlights_by_parent.keys().cloned() {
        // Find the parent document
        let parent = documents
            .iter()
            .find(|d| d.id == parent_id)
            .expect("Parent document must exist since we got its ID from highlights_by_parent");

        let full_url = parent.source_url.clone();
        let filename = if duplicate_titles.contains(&parent.title) {
            get_new_entry_filename(&parent.title, Some(&full_url))
        } else {
            get_new_entry_filename(&parent.title, None)
        };

        let uuid = uuid::Uuid::new_v4().to_string();
        // Skip if file already exists in org-roam
        if existing_refs
            .get(&parent.roam_db_ref)
            .and_then(|id| existing_nodes.get(id))
            .is_some()
        {
            continue;
        }

        let mut context = Context::new();
        context.insert("uuid", &uuid);
        context.insert("roam_ref", &parent.roam_full_ref);
        if parent.has_url {
            context.insert("full_url", &full_url);
        }
        context.insert("readwise_url", &parent.readwise_url);
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
                .rev() // Reverse the order of highlights so they end up in the correct order in the org file
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
        let content = tera.render("document.org.tera", &context)?;
        std::fs::write(&filename, &content)?;
        println!("Created file: {}", filename);
        documents_processed += 1;
    }
    println!("\nProcessed {} documents", documents_processed);
    let duration = start_time.elapsed();
    println!("Time taken: {:?}", duration);
    Ok(())
}

fn get_new_entry_filename(title: &str, url: Option<&str>) -> String {
    // Generate a new filename for a new org-roam entry, based on the title.
    // If the URL is provided, also include the first 8 characters of the MD5 hash of the URL in the filename.
    let now = chrono::Local::now();
    let home_dir = std::env::var("HOME").expect("HOME environment variable not set");
    let slug = slug::slugify(title);
    let truncated_slug = if slug.len() > 100 {
        slug[..100].to_string()
    } else {
        slug
    };

    let maybe_url_part = if let Some(u) = url {
        let hash = md5::compute(u);
        let hash_str = format!("{:08x}", hash);
        let truncated_hash = &hash_str[..8];
        format!("-{}", truncated_hash)
    } else {
        String::new()
    };
    format!(
        "{}/org/roam/{}-{}{}.org",
        home_dir,
        now.format("%Y%m%d%H%M%S"),
        truncated_slug,
        maybe_url_part
    )
}

fn get_duplicate_titles(documents: &[Document]) -> Vec<String> {
    // Return a list of titles that appear more than once in the document list
    let mut title_counts: HashMap<String, u32> = HashMap::new();
    for document in documents {
        *title_counts.entry(document.title.clone()).or_default() += 1;
    }
    title_counts
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(title, _)| title.clone())
        .collect()
}
