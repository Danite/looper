use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const CONFIG_FILENAME: &str = "looper.config.json";

/// Project-level configuration loaded from looper.config.json.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub categories: HashMap<String, CategoryConfig>,
    #[serde(default)]
    pub min_confidence: Option<String>,
    #[serde(default)]
    pub manifest: Option<String>,
}

/// Per-category configuration overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CategoryConfig {
    #[serde(default)]
    pub ignore: Vec<String>,
}

impl Config {
    /// Generate a default config with sensible defaults.
    pub fn default_config() -> Self {
        let mut categories = HashMap::new();
        categories.insert(
            "env_var".into(),
            CategoryConfig {
                ignore: vec!["NODE_ENV".into(), "CI".into()],
            },
        );
        categories.insert(
            "network".into(),
            CategoryConfig {
                ignore: vec!["localhost".into(), "127.0.0.1".into()],
            },
        );

        Self {
            include: vec!["src/**".into(), "lib/**".into()],
            exclude: vec![
                "**/*.test.ts".into(),
                "**/*.test.js".into(),
                "**/*.spec.ts".into(),
                "**/*.spec.js".into(),
                "**/node_modules/**".into(),
                "**/dist/**".into(),
                "**/build/**".into(),
            ],
            categories,
            min_confidence: Some("low".into()),
            manifest: Some("shadow-deps.json".into()),
        }
    }

    /// Serialize to pretty JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Load config from an explicit path or by auto-detecting looper.config.json.
pub fn load_config(explicit_path: Option<&Path>) -> Option<Config> {
    let path = match explicit_path {
        Some(p) => {
            if p.exists() {
                Some(p.to_path_buf())
            } else {
                None
            }
        }
        None => find_config_file(&std::env::current_dir().ok()?),
    };

    let path = path?;
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Walk up from `start_dir` looking for looper.config.json.
fn find_config_file(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = start_dir.to_path_buf();
    loop {
        let candidate = dir.join(CONFIG_FILENAME);
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Write a default config file to the given directory.
pub fn write_default_config(dir: &Path) -> std::io::Result<PathBuf> {
    let config = Config::default_config();
    let json = config
        .to_json()
        .map_err(std::io::Error::other)?;
    let path = dir.join(CONFIG_FILENAME);
    std::fs::write(&path, json)?;
    Ok(path)
}
