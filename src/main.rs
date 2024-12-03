mod readwise_api;
mod settings;
mod util;

use readwise_api::*;
use settings::SETTINGS;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use tera::{Context, Tera};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start_time = std::time::Instant::now();
    let tera = Tera::new(&SETTINGS.templates_dir.to_string_lossy())?;
    let org_roam_dir = &SETTINGS.org_roam_dir;
    let existing_refs = get_existing_refs(org_roam_dir)?;
    let documents = get_document_list().await?;
    let highlights = get_highlight_list().await?;
    let notes = get_note_list().await?;

    let highlights_by_parent = map_parents_to_highlights(documents.clone(), highlights);
    let notes_by_parent = note_list_to_map(notes);

    let duplicate_titles = get_duplicate_titles(&documents);
    println!("Duplicate titles: {:?}", duplicate_titles);

    let mut files_created = 0;
    for parent_id in highlights_by_parent.keys().cloned() {
        // Find the parent document
        let parent = documents
            .iter()
            .find(|d| d.id == parent_id)
            .expect("Parent document must exist since we got its ID from highlights_by_parent");

        // Skip if file already exists in org-roam
        if existing_refs.contains_key(&parent.roam_ref) {
            println!(
                "Filename already exists: {}",
                existing_refs[&parent.roam_ref]
            );
            continue;
        }

        let filename = if duplicate_titles.contains(&parent.title) {
            get_new_entry_filename(org_roam_dir, &parent.title, Some(&parent.source_url))
        } else {
            get_new_entry_filename(org_roam_dir, &parent.title, None)
        };

        let uuid = uuid::Uuid::new_v4().to_string();

        let mut context = Context::new();
        context.insert("uuid", &uuid);
        context.insert("roam_ref", &parent.roam_ref);
        if parent.has_url {
            context.insert("full_url", &parent.source_url);
        }
        context.insert("readwise_url", &parent.readwise_url);
        context.insert("title", &parent.title);
        context.insert(
            "today",
            &chrono::Local::now().format("%Y-%m-%d %a").to_string(),
        );
        context.insert(
            "read_status",
            if parent.location.as_str() == "archive" {
                "DONE"
            } else {
                "TODO"
            },
        );
        let highlights_with_notes =
            get_highlights_with_notes(&highlights_by_parent, &notes_by_parent, &parent_id);
        if !highlights_with_notes.is_empty() {
            context.insert("highlights", &highlights_with_notes);
        }
        let content = tera.render("document.org.tera", &context)?;
        std::fs::write(&filename, &content)?;
        println!("Created file: {}", filename);
        files_created += 1;
    }
    println!("\nCreated {} files", files_created);
    let duration = start_time.elapsed();
    println!("Time taken: {:?}", duration);
    Ok(())
}

fn get_existing_refs(
    org_roam_dir: &Path,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    // Run ripgrep to find all ROAM_REFS lines in org_roam_dir.
    // Return a mapping from roam_ref to full filename.
    let output = Command::new("rg")
        .args([
            "--with-filename",
            "^:ROAM_REFS:",
            &org_roam_dir.to_string_lossy(),
        ])
        .output()?;

    let output_str = String::from_utf8(output.stdout)?;

    // Parse the output into a map of roam_ref -> filename
    let mut refs_map = HashMap::new();
    for line in output_str.lines() {
        // Each line is in the format: filename::ROAM_REFS: ref
        if let Some((filename, roam_ref)) = line.split_once("::ROAM_REFS:") {
            let roam_ref = roam_ref.trim().to_string();
            refs_map.insert(roam_ref, filename.to_string());
        }
    }
    Ok(refs_map)
}

fn get_new_entry_filename(org_roam_dir: &Path, title: &str, url: Option<&str>) -> String {
    // Generate a new filename for a new org-roam entry, based on the title.
    // If the URL is provided, also include the first 8 characters of the MD5 hash of the URL in the filename.
    let now = chrono::Local::now();
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
    org_roam_dir
        .join(format!(
            "{}-{}{}.org",
            now.format("%Y%m%d%H%M%S"),
            truncated_slug,
            maybe_url_part
        ))
        .to_string_lossy()
        .into_owned()
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

fn get_highlights_with_notes(
    highlights_by_parent: &HashMap<String, Vec<Highlight>>,
    notes_by_parent: &HashMap<String, Note>,
    parent_id: &str,
) -> Vec<serde_json::Value> {
    let highlights = highlights_by_parent.get(parent_id).unwrap();
    highlights
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
        .collect()
}
