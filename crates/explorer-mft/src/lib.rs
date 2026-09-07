//! Internal MFT subsystem shared by the application, helper, and service.

use std::path::{Path, PathBuf};

/// Fixed SuperExplorer MFT SQLite cache: `%ProgramData%\SuperExplorer\MftIndex`.
#[must_use]
pub fn mft_index_cache_root() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("SuperExplorer")
        .join("MftIndex")
}

#[must_use]
pub fn mft_sqlite_path(letter: char) -> PathBuf {
    mft_index_cache_root().join(format!("{}.mft.sqlite3", letter.to_ascii_uppercase()))
}

#[must_use]
pub fn volume_drive_letter(path: &Path) -> Option<char> {
    let text = path.to_string_lossy();
    let mut chars = text.chars();
    let letter = chars.next()?;
    let colon = chars.next()?;
    (colon == ':' && letter.is_ascii_alphabetic()).then(|| letter.to_ascii_uppercase())
}

#[path = "../../explorer-app/src/mft_focus.rs"]
pub mod mft_focus;
#[path = "../../explorer-app/src/mft_journal.rs"]
pub mod mft_journal;
#[path = "../../explorer-app/src/mft_migration.rs"]
pub mod mft_migration;
#[path = "../../explorer-app/src/mft_persistence.rs"]
pub mod mft_persistence;
#[path = "../../explorer-app/src/mft_query.rs"]
pub mod mft_query;
#[path = "../../explorer-app/src/mft_runtime.rs"]
pub mod mft_runtime;
#[path = "../../explorer-app/src/mft_size_map.rs"]
pub mod mft_size_map;
#[path = "../../explorer-app/src/mft_sqlite.rs"]
pub mod mft_sqlite;
