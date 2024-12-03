use config::{Config, File};
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub org_roam_dir: PathBuf,
    pub templates_dir: PathBuf,
    pub updated_after_file_path: PathBuf,
}

pub static SETTINGS: Lazy<Settings> = Lazy::new(|| {
    let config = Config::builder()
        .add_source(File::with_name("config/config.toml"))
        .build()
        .unwrap();

    let home_dir = std::env::var("HOME").expect("HOME environment variable not set");
    let mut settings = config.try_deserialize::<Settings>().unwrap();

    // Expand ~ to home directory for all PathBuf fields
    for path in [
        &mut settings.org_roam_dir,
        &mut settings.templates_dir,
        &mut settings.updated_after_file_path,
    ] {
        if path.starts_with("~") {
            *path = PathBuf::from(&home_dir).join(path.strip_prefix("~").unwrap());
        }
    }
    settings
});
