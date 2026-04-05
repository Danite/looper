use crate::types::ShadowDep;
use std::path::Path;

/// Generate a Dockerfile from Binary and Port shadow deps.
pub fn generate_dockerfile(_deps: &[ShadowDep], _project_root: &Path) -> String {
    todo!("dockerfile generator not yet implemented")
}
