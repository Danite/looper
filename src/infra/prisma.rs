use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

static ENV_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"env\(\s*"([^"]+)"\s*\)"#).unwrap());

#[derive(Debug, Clone)]
pub struct PrismaEnvEntry {
    pub name: String,
    pub line: usize,
    pub source_file: String,
}

#[derive(Debug, Default)]
pub struct PrismaResult {
    pub entries: HashMap<String, PrismaEnvEntry>,
    pub source_file: Option<String>,
}

pub fn parse_prisma_file(path: &Path) -> Result<PrismaResult, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;

    let source_file = path.to_string_lossy().into_owned();
    let mut result = PrismaResult {
        entries: HashMap::new(),
        source_file: Some(source_file.clone()),
    };

    for (line_num, line) in content.lines().enumerate() {
        let line_num = line_num + 1;
        for caps in ENV_RE.captures_iter(line) {
            if let Some(name) = caps.get(1) {
                let name = name.as_str().to_string();
                result
                    .entries
                    .entry(name.clone())
                    .or_insert(PrismaEnvEntry {
                        name,
                        line: line_num,
                        source_file: source_file.clone(),
                    });
            }
        }
    }

    Ok(result)
}

const PRISMA_FILE_NAMES: &[&str] = &["prisma/schema.prisma", "schema.prisma"];

pub fn find_and_parse_prisma(project_root: &Path, warnings: &mut Vec<String>) -> PrismaResult {
    for name in PRISMA_FILE_NAMES {
        let path = project_root.join(name);
        if path.exists() {
            match parse_prisma_file(&path) {
                Ok(result) => return result,
                Err(e) => warnings.push(format!("failed to parse {}: {e}", path.display())),
            }
        }
    }
    PrismaResult::default()
}
