use config::{Config, File};
use serde::Deserialize;
use std::path::PathBuf;
#[derive(Debug, Deserialize)]
pub struct Settings {
    pub org_roam_dir: PathBuf,
    pub templates_dir: PathBuf,
}

impl Settings {
    pub fn new() -> Result<Self, config::ConfigError> {
        let config = Config::builder()
            // Add default values
            .add_source(File::with_name("config/config.toml"))
            .build()
            .unwrap();

        let home_dir = std::env::var("HOME").expect("HOME environment variable not set");
        let mut settings = config.try_deserialize::<Settings>().unwrap();
        if settings.org_roam_dir.starts_with("~") {
            settings.org_roam_dir =
                PathBuf::from(home_dir).join(settings.org_roam_dir.strip_prefix("~").unwrap());
        }
        Ok(settings)
    }
}
