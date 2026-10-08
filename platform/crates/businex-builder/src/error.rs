//! Failures from the isolated app build pipeline.

/// One compiler diagnostic, in the shape the platform surfaces to app
/// authors. Provider or host detail never leaks through this type.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub code: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// The submitted project is unsafe, oversized or unsupported.
    #[error("invalid project input: {0}")]
    InvalidInput(String),
    /// The builder image is missing or cannot be built.
    #[error("builder image unavailable: {0}")]
    Image(String),
    /// The isolation tooling itself failed (docker errors, bad output).
    #[error("build tool failed: {0}")]
    Tool(String),
    /// Type checking or compilation failed with these diagnostics.
    #[error("compilation failed with {} error(s)", .0.len())]
    Compile(Vec<Diagnostic>),
    /// The build or handler run exceeded its wall-clock budget and was
    /// killed; the container is removed, nothing keeps running.
    #[error("isolated build timed out after {0}s")]
    Timeout(u64),
}
