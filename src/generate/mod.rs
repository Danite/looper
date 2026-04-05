pub mod compose;
pub mod dockerfile;
pub mod env;
pub mod readme;

use crate::types::ShadowDep;

/// Target infrastructure file to generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerateTarget {
    Env,
    Compose,
    Dockerfile,
    Readme,
}

impl GenerateTarget {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "env" => Some(Self::Env),
            "compose" => Some(Self::Compose),
            "dockerfile" => Some(Self::Dockerfile),
            "readme" => Some(Self::Readme),
            _ => None,
        }
    }

    pub fn default_filename(&self) -> &'static str {
        match self {
            Self::Env => ".env.example",
            Self::Compose => "docker-compose.yml",
            Self::Dockerfile => "Dockerfile",
            Self::Readme => "REQUIREMENTS.md",
        }
    }

    pub fn all() -> &'static [GenerateTarget] {
        &[Self::Env, Self::Compose, Self::Dockerfile, Self::Readme]
    }
}

/// Generate output for a specific target from shadow deps.
pub fn generate(target: GenerateTarget, deps: &[ShadowDep], project_root: &std::path::Path) -> String {
    match target {
        GenerateTarget::Env => env::generate_env(deps),
        GenerateTarget::Compose => compose::generate_compose(deps),
        GenerateTarget::Dockerfile => dockerfile::generate_dockerfile(deps, project_root),
        GenerateTarget::Readme => readme::generate_readme(deps),
    }
}
