use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// A single shadow dependency finding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShadowDep {
    pub id: String,
    pub category: Category,
    pub name: String,
    pub confidence: Confidence,
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
    pub locations: Vec<Location>,
}

impl ShadowDep {
    pub fn new(
        category: Category,
        name: impl Into<String>,
        confidence: Confidence,
        location: Location,
    ) -> Self {
        let name = name.into();
        let id = format!("{category}::{name}");
        Self {
            id,
            category,
            name,
            confidence,
            metadata: Metadata::default(),
            locations: vec![location],
        }
    }

    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Merge locations from another ShadowDep with the same id.
    pub fn merge_locations(&mut self, other: &ShadowDep) {
        debug_assert_eq!(self.id, other.id);
        for loc in &other.locations {
            if !self.locations.contains(loc) {
                self.locations.push(loc.clone());
            }
        }
    }
}

impl fmt::Display for ShadowDep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let loc = self
            .locations
            .first()
            .map(|l| l.to_string())
            .unwrap_or_default();
        write!(f, "{:<20} {:<25} {}", self.name, loc, self.confidence)
    }
}

/// The six detection categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    EnvVar,
    Filesystem,
    Network,
    Binary,
    Port,
    Platform,
}

impl Category {
    pub fn label(&self) -> &'static str {
        match self {
            Self::EnvVar => "ENV VARS",
            Self::Filesystem => "FILESYSTEM",
            Self::Network => "NETWORK",
            Self::Binary => "BINARIES",
            Self::Port => "PORTS",
            Self::Platform => "PLATFORM",
        }
    }

    /// CLI filter name used in --only / --exclude.
    pub fn filter_name(&self) -> &'static str {
        match self {
            Self::EnvVar => "env",
            Self::Filesystem => "fs",
            Self::Network => "network",
            Self::Binary => "shell",
            Self::Port => "ports",
            Self::Platform => "platform",
        }
    }

    pub fn from_filter_name(name: &str) -> Option<Category> {
        Self::all()
            .iter()
            .find(|c| c.filter_name() == name)
            .copied()
    }

    pub fn all() -> &'static [Category] {
        &[
            Self::EnvVar,
            Self::Filesystem,
            Self::Network,
            Self::Binary,
            Self::Port,
            Self::Platform,
        ]
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::EnvVar => "env_var",
            Self::Filesystem => "filesystem",
            Self::Network => "network",
            Self::Binary => "binary",
            Self::Port => "port",
            Self::Platform => "platform",
        };
        f.write_str(s)
    }
}

/// Confidence level for a detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl std::str::FromStr for Confidence {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            _ => Err(format!("unknown confidence level: {s}")),
        }
    }
}

impl Confidence {
    pub fn parse(s: &str) -> Option<Confidence> {
        s.parse().ok()
    }
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        };
        f.write_str(s)
    }
}

/// Source location of a finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub file: String,
    pub line: u32,
    pub col: u32,
}

impl Location {
    pub fn new(file: impl Into<String>, line: u32, col: u32) -> Self {
        Self {
            file: file.into(),
            line,
            col,
        }
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.col)
    }
}

/// Free-form metadata attached to a shadow dep.
/// Uses a HashMap for flexibility across categories.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Metadata {
    pub entries: HashMap<String, serde_json::Value>,
}

impl Metadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) {
        self.entries.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.entries.get(key)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.entries.get(key).and_then(|v| v.as_str())
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.entries.get(key).and_then(|v| v.as_bool())
    }
}

/// Top-level scan result — the full manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanManifest {
    pub schema_version: String,
    pub tool_version: String,
    pub scanned_at: String,
    pub root: String,
    pub files_scanned: usize,
    pub scan_duration_ms: u64,
    pub deps: Vec<ShadowDep>,
}

impl ScanManifest {
    pub fn new(root: impl Into<String>, files_scanned: usize, scan_duration_ms: u64) -> Self {
        Self {
            schema_version: "1".into(),
            tool_version: env!("CARGO_PKG_VERSION").into(),
            scanned_at: chrono::Utc::now().to_rfc3339(),
            root: root.into(),
            files_scanned,
            scan_duration_ms,
            deps: Vec::new(),
        }
    }
}

/// Convert a byte offset in source text to a (line, col) pair (1-indexed).
pub fn line_col(source: &str, offset: u32) -> (u32, u32) {
    let mut line = 1u32;
    let mut col = 1u32;
    for &byte in source.as_bytes().iter().take(offset as usize) {
        if byte == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
