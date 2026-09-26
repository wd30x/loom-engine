//! Helpers shared by everything that writes output files (screenshots,
//! state dumps, recordings).

use std::path::Path;

use anyhow::{Context, Result};

/// Creates the parent directories of an output file, if it has any.
pub(crate) fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }
    Ok(())
}
