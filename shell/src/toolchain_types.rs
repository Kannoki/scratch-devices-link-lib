//! Shared types used by both toolchain extraction helpers and toolchain release management.

use std::sync::Arc;

/// Setup-phase + progress payload.
#[derive(Debug, Clone)]
pub struct SetupProgress {
    pub phase: String,
    pub progress: u8,
}

/// Shared progress sink. Cloneable so it can be captured by async closures.
pub type ProgressFn = Arc<dyn Fn(SetupProgress) + Send + Sync + 'static>;
