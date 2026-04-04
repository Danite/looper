use super::{Detector, DetectorContext};
use crate::types::ShadowDep;

/// Detects environment variable access patterns.
/// Full implementation in looper-73o.
pub struct EnvVarsDetector;

impl Detector for EnvVarsDetector {
    fn name(&self) -> &str {
        "env_vars"
    }

    fn detect(&self, _ctx: &DetectorContext<'_>) -> Vec<ShadowDep> {
        Vec::new()
    }
}
