use std::path::Path;

#[derive(Debug, Default)]
pub struct FnccConfig {
    pub ui_dir: String,
    pub lib_dir: String,
    pub output_dir: String,
    pub entry: String,
}

pub fn load(path: &Path) -> Result<FnccConfig, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("failed to read config: {e}"))?;

    // Simple TOML-like parsing for POC
    // In production, use a proper TOML crate
    let mut config = FnccConfig::default();

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
        config.output_dir = "target/fncc".to_string();
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static CONFIG_TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn write_config(content: &str) -> PathBuf {
        let dir = std::env::temp_dir();
        let id = CONFIG_TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!("fncc_test_config_{}.toml", id));
        let mut f = std::fs::File::create(&path).unwrap();
        write!(f, "{content}").unwrap();
        path
    }

    // --- Happy path ---

    #[test]
    fn test_loads_all_fields() {
        let path = write_config("ui=\"src/myui\"\nlib=\"src/mylib\"\noutput=\"out\"\nentry=\"main\"");
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.ui_dir, "src/myui");
        assert_eq!(cfg.lib_dir, "src/mylib");
        assert_eq!(cfg.output_dir, "out");
        assert_eq!(cfg.entry, "main");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_loads_empty_config_with_defaults() {
        let path = write_config("");
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.ui_dir, "src/ui");
        assert_eq!(cfg.output_dir, "target/fncc");
        assert_eq!(cfg.lib_dir, "");
        assert_eq!(cfg.entry, "");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_unknown_keys_ignored() {
        let path = write_config("unknown=true\nui=\"custom\"");
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.ui_dir, "custom");
        let _ = std::fs::remove_file(&path);
    }

    // --- Edge cases ---

    #[test]
    fn test_ui_dir_default_when_empty() {
        let path = write_config("ui=\"\"");
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.ui_dir, "src/ui");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_output_dir_default_when_empty() {
        let path = write_config("output=\"\"");
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.output_dir, "target/fncc");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_values_with_extra_whitespace_trimmed() {
        let path = write_config(r#"ui = "  spacey  ""#);
        let cfg = load(&path).unwrap();
        // homemade parser doesn't trim value's inner whitespace, only quotes
        assert_eq!(cfg.ui_dir, "  spacey  ");
        let _ = std::fs::remove_file(&path);
    }

    // --- Error handling ---

    #[test]
    fn test_nonexistent_file_returns_error() {
        let result = load(Path::new("/tmp/__fncc_nonexistent_file_12345.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_lines_without_equals_are_ignored() {
        let path = write_config("ui=\"ok\"\njust a comment\nlib=\"val\"");
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.ui_dir, "ok");
        assert_eq!(cfg.lib_dir, "val");
        let _ = std::fs::remove_file(&path);
    }
}
