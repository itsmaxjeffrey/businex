//! Source project input for one custom TypeScript app.
//!
//! Every path is treated as hostile: relative, extension allowlisted, no
//! traversal, no hidden files. Size caps keep one submission from filling
//! the builder volume. npm dependencies are refused explicitly for now
//! because vendoring is not implemented; the pipeline fails closed instead
//! of silently building without them.

use crate::error::BuildError;
use std::collections::BTreeSet;

pub const MAX_FILES: usize = 64;
pub const MAX_FILE_BYTES: usize = 256 * 1024;
pub const MAX_TOTAL_BYTES: usize = 2 * 1024 * 1024;
pub const ENTRY_FILE: &str = "app.ts";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceProject {
    files: Vec<SourceFile>,
    dependencies: Vec<String>,
}

impl SourceProject {
    pub fn new(
        files: Vec<SourceFile>,
        dependencies: Vec<String>,
    ) -> Result<SourceProject, BuildError> {
        if files.is_empty() {
            return Err(BuildError::InvalidInput("project has no files".into()));
        }
        if files.len() > MAX_FILES {
            return Err(BuildError::InvalidInput(format!(
                "project has {} files, the limit is {}",
                files.len(),
                MAX_FILES
            )));
        }
        let mut seen = BTreeSet::new();
        let mut total = 0usize;
        for file in &files {
            safe_relative_path(&file.path)?;
            let lower = file.path.to_ascii_lowercase();
            if !(lower.ends_with(".ts") || lower.ends_with(".json")) {
                return Err(BuildError::InvalidInput(format!(
                    "unsupported file type: {}",
                    file.path
                )));
            }
            if !seen.insert(file.path.clone()) {
                return Err(BuildError::InvalidInput(format!(
                    "duplicate file path: {}",
                    file.path
                )));
            }
            if file.content.contains('\0') {
                return Err(BuildError::InvalidInput(format!(
                    "file contains NUL bytes: {}",
                    file.path
                )));
            }
            if file.content.len() > MAX_FILE_BYTES {
                return Err(BuildError::InvalidInput(format!(
                    "file exceeds {} bytes: {}",
                    MAX_FILE_BYTES, file.path
                )));
            }
            total += file.content.len();
        }
        if total > MAX_TOTAL_BYTES {
            return Err(BuildError::InvalidInput(format!(
                "project exceeds {} bytes in total",
                MAX_TOTAL_BYTES
            )));
        }
        if !seen.contains(ENTRY_FILE) {
            return Err(BuildError::InvalidInput(format!(
                "project must contain {}",
                ENTRY_FILE
            )));
        }
        if !dependencies.is_empty() {
            return Err(BuildError::InvalidInput(
                "npm dependencies are not supported yet (dependency vendoring is not implemented)"
                    .into(),
            ));
        }
        Ok(SourceProject {
            files,
            dependencies,
        })
    }

    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }
}

/// Relative, single-case-safe path with no traversal and no hidden files.
/// Used for both submitted sources and produced artifacts.
pub fn safe_relative_path(path: &str) -> Result<(), BuildError> {
    if path.is_empty() {
        return Err(BuildError::InvalidInput("empty file path".into()));
    }
    if path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return Err(BuildError::InvalidInput(format!(
            "unsafe file path: {}",
            path
        )));
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(BuildError::InvalidInput(format!(
                "unsafe file path: {}",
                path
            )));
        }
        if segment.starts_with('.') {
            return Err(BuildError::InvalidInput(format!(
                "hidden files are not allowed: {}",
                path
            )));
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
        {
            return Err(BuildError::InvalidInput(format!(
                "unsafe file path: {}",
                path
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, content: &str) -> SourceFile {
        SourceFile {
            path: path.into(),
            content: content.into(),
        }
    }

    fn ok() -> Vec<SourceFile> {
        vec![file("app.ts", "export default {};")]
    }

    #[test]
    fn entry_file_is_required() {
        let err = SourceProject::new(vec![file("util.ts", "x")], vec![]).unwrap_err();
        assert!(matches!(err, BuildError::InvalidInput(_)), "{:?}", err);
    }

    #[test]
    fn traversal_absolute_hidden_and_backslash_paths_are_refused() {
        for path in [
            "../escape.ts",
            "a/../b.ts",
            "/etc/passwd.ts",
            "lib\\win.ts",
            ".hidden.ts",
            "lib/.secret.ts",
            "lib//gap.ts",
            "bad name.ts",
            "app.ts\0",
        ] {
            let err = SourceProject::new(vec![file(path, "x"), file("app.ts", "x")], vec![])
                .expect_err(path);
            assert!(matches!(err, BuildError::InvalidInput(_)), "{:?}", err);
        }
    }

    #[test]
    fn unsupported_extensions_duplicates_and_sizes_are_refused() {
        let err =
            SourceProject::new(vec![file("app.js", "x"), file("app.ts", "x")], vec![])
                .unwrap_err();
        assert!(matches!(err, BuildError::InvalidInput(_)));

        let err = SourceProject::new(
            vec![file("app.ts", "x"), file("app.ts", "y")],
            vec![],
        )
        .unwrap_err();
        assert!(matches!(err, BuildError::InvalidInput(_)));

        let huge = "x".repeat(MAX_FILE_BYTES + 1);
        let err = SourceProject::new(vec![file("app.ts", &huge)], vec![]).unwrap_err();
        assert!(matches!(err, BuildError::InvalidInput(_)));
    }

    #[test]
    fn npm_dependencies_fail_closed_for_now() {
        let err = SourceProject::new(ok(), vec!["lodash".into()]).unwrap_err();
        assert!(matches!(err, BuildError::InvalidInput(_)), "{:?}", err);
    }

    #[test]
    fn valid_nested_project_is_accepted() {
        let project = SourceProject::new(
            vec![
                file("app.ts", "export default {};"),
                file("lib/util.ts", "export const n = 1;"),
                file("data.json", "{}"),
            ],
            vec![],
        )
        .expect("valid project");
        assert_eq!(project.files().len(), 3);
    }
}
