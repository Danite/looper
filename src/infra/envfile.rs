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

/// Auto-detect and parse .env files with precedence: .env > .env.example
pub fn find_and_parse_env(project_root: &Path) -> EnvFileResult {
    let env_path = project_root.join(".env");
    let example_path = project_root.join(".env.example");

    if env_path.exists() {
        if let Ok(result) = parse_env_file(&env_path) {
            return result;
        }
    }

    if example_path.exists() {
        if let Ok(result) = parse_env_file(&example_path) {
            return result;
        }
    }

    EnvFileResult::default()
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
