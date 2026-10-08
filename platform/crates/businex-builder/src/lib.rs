//! Isolated build and execution of custom TypeScript apps.
//!
//! Generated and hand-written app code is never compiled or run on the
//! host: every build and every handler call happens inside a throwaway
//! container with no network, no capabilities, a read-only root filesystem
//! and hard CPU, memory, process and time limits. Apps reach company data
//! only through the typed businex SDK surface (see assets/sdk.d.ts).
//!
//! The compiled artifact is content addressed: Artifact::hash digests
//! every output file by path and content. That digest is the compiled
//! artifact proof distinct from the manifest hash.

pub mod artifact;
pub mod error;
pub mod project;
pub mod runner;

pub use artifact::{Artifact, ArtifactFile};
pub use error::{BuildError, Diagnostic};
pub use project::{SourceFile, SourceProject};
pub use runner::{BuildOutput, DockerIsolation, IsolationConfig, IsolationOutput, RunOutcome};
