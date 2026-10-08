//! Content-addressed compiled artifacts.
//!
//! The artifact hash is the compiled-artifact proof: it digests every
//! output file by path and content, so the same sources always hash the
//! same and any byte change moves the hash. It is never presented as a
//! manifest hash.

use crate::error::BuildError;
use crate::project::safe_relative_path;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Artifact {
    pub files: Vec<ArtifactFile>,
}

impl Artifact {
    /// Read an output directory into an artifact. Paths are re-validated:
    /// whatever the build produced is still treated as untrusted.
    pub fn from_dir(dir: &Path) -> Result<Artifact, BuildError> {
        let mut files = Vec::new();
        collect(dir, dir, &mut files)?;
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Artifact { files })
    }

    /// SHA-256 over the sorted per-file digests. Stable across machines and
    /// runs, sensitive to any content or path change.
    pub fn hash(&self) -> String {
        let mut files: Vec<&ArtifactFile> = self.files.iter().collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let mut outer = Sha256::new();
        for file in files {
            let inner = Sha256::digest(&file.bytes);
            outer.update(file.path.as_bytes());
            outer.update(b"\n");
            outer.update(hex::encode(inner).as_bytes());
            outer.update(b"\n");
        }
        hex::encode(outer.finalize())
    }
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<ArtifactFile>) -> Result<(), BuildError> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        BuildError::Tool(format!("cannot read build output {}: {}", dir.display(), e))
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| BuildError::Tool(format!("bad output entry: {}", e)))?;
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, out)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| BuildError::Tool("output escaped the artifact root".into()))?;
        let relative = relative
            .to_str()
            .ok_or_else(|| BuildError::Tool("non-UTF-8 output path".into()))?
            .replace('\\', "/");
        safe_relative_path(&relative)?;
        let bytes = std::fs::read(&path)
            .map_err(|e| BuildError::Tool(format!("cannot read output file: {}", e)))?;
        out.push(ArtifactFile {
            path: relative,
            bytes,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(entries: &[(&str, &[u8])]) -> Artifact {
        Artifact {
            files: entries
                .iter()
                .map(|(path, bytes)| ArtifactFile {
                    path: (*path).into(),
                    bytes: (*bytes).to_vec(),
                })
                .collect(),
        }
    }

    #[test]
    fn hash_is_deterministic_and_order_independent() {
        let a = artifact(&[("app.js", b"one"), ("lib/util.js", b"two")]);
        let b = artifact(&[("lib/util.js", b"two"), ("app.js", b"one")]);
        assert_eq!(a.hash(), b.hash());
        assert_eq!(a.hash(), a.clone().hash());
    }

    #[test]
    fn hash_moves_with_any_content_or_path_change() {
        let base = artifact(&[("app.js", b"one")]);
        let content = artifact(&[("app.js", b"onf")]);
        let path = artifact(&[("app2.js", b"one")]);
        let extra = artifact(&[("app.js", b"one"), ("x.js", b"")]);
        assert_ne!(base.hash(), content.hash());
        assert_ne!(base.hash(), path.hash());
        assert_ne!(base.hash(), extra.hash());
    }

    #[test]
    fn produced_paths_are_still_validated() {
        std::fs::create_dir_all("/tmp/businex-artifact-test/sub").unwrap();
        std::fs::write("/tmp/businex-artifact-test/sub/app.js", b"x").unwrap();
        std::fs::write("/tmp/businex-artifact-test/sub/.hidden.js", b"x").unwrap();
        let err = Artifact::from_dir(Path::new("/tmp/businex-artifact-test/sub")).unwrap_err();
        assert!(matches!(err, BuildError::InvalidInput(_)), "{:?}", err);
        let _ = std::fs::remove_dir_all("/tmp/businex-artifact-test");
    }
}
