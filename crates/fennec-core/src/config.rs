use std::path::Path;

#[derive(Debug, Default)]
pub struct FennecConfig {
    pub ui_dir: String,
    pub lib_dir: String,
    pub output_dir: String,
    pub entry: String,
}

pub fn load(path: &Path) -> Result<FennecConfig, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read config: {e}"))?;

    // Simple TOML-like parsing for POC
    // In production, use a proper TOML crate
    let mut config = FennecConfig::default();

    for line in content.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            match key {
                "ui" => config.ui_dir = value.to_string(),
                "lib" => config.lib_dir = value.to_string(),
                "output" => config.output_dir = value.to_string(),
                "entry" => config.entry = value.to_string(),
                _ => {}
            }
        }
    }

    // apply defaults
    if config.ui_dir.is_empty() {
        config.ui_dir = "src/ui".to_string();
    }
    if config.output_dir.is_empty() {
        config.output_dir = "target/fennec".to_string();
    }

    Ok(config)
}
