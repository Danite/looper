use std::collections::HashMap;
use std::path::Path;

/// A parsed env var with its source location.
#[derive(Debug, Clone)]
pub struct EnvEntry {
    pub name: String,
    pub line: usize,
    pub source_file: String,
}

/// Result of parsing one or more .env files.
#[derive(Debug, Default)]
pub struct EnvFileResult {
    pub entries: HashMap<String, EnvEntry>,
    pub source_file: Option<String>,
}

impl EnvFileResult {
    pub fn var_names(&self) -> std::collections::HashSet<String> {
        self.entries.keys().cloned().collect()
    }

    pub fn get_source(&self, name: &str) -> Option<String> {
        self.entries
            .get(name)
            .map(|e| format!("{} line {}", e.source_file, e.line))
    }
}

/// Parse a .env file, returning variable names with line numbers.
pub fn parse_env_file(path: &Path) -> Result<EnvFileResult, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;

    let source_file = path.to_string_lossy().into_owned();
    let mut result = EnvFileResult {
        entries: HashMap::new(),
        source_file: Some(source_file.clone()),
    };

    for (line_num, line) in content.lines().enumerate() {
        let line_num = line_num + 1;
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(name) = extract_var_name(trimmed) {
            if !name.is_empty() {
                result.entries.insert(
                    name.clone(),
                    EnvEntry {
                        name,
                        line: line_num,
                        source_file: source_file.clone(),
                    },
                );
            }
        }
    }

    Ok(result)
}

/// Known .env file names in descending priority order.
/// Higher-priority files are listed first; their entries win on conflict.
const ENV_FILE_PRIORITY: &[&str] = &[
    ".env.development.local",
    ".env.test.local",
    ".env.production.local",
    ".env.local",
    ".env.development",
    ".env.test",
    ".env.production",
    ".env",
    ".env.defaults",
    ".env.example",
    ".env.sample",
];

/// Auto-detect and parse .env files, merging entries from all found files.
/// Lower-priority files are parsed first so higher-priority entries override.
pub fn find_and_parse_env(project_root: &Path) -> EnvFileResult {
    let mut merged = EnvFileResult::default();

    for name in ENV_FILE_PRIORITY {
        let path = project_root.join(name);
        if path.exists() {
            if let Ok(result) = parse_env_file(&path) {
                if merged.source_file.is_none() {
                    merged.source_file = result.source_file;
                }
                for (key, entry) in result.entries {
                    merged.entries.entry(key).or_insert(entry);
                }
            }
        }
    }

    merged
}

fn extract_var_name(line: &str) -> Option<String> {
    let line = line.strip_prefix("export ").unwrap_or(line).trim_start();

    let eq_pos = line.find('=')?;
    let name = line[..eq_pos].trim();

    if name.is_empty() || !is_valid_env_name(name) {
        return None;
    }

    Some(name.to_string())
}

fn is_valid_env_name(name: &str) -> bool {
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.chars().next().is_some_and(|c| !c.is_ascii_digit())
}
