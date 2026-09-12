//! First-launch import of Windows File Explorer pinned folders into Quick Access.

use explorer_model::PersistedQuickAccessPin;

/// Uses stored Quick Access when a session already exists.
///
/// A missing session (first launch after install) receives the imported Explorer pins.
pub(crate) fn seed_quick_access_for_first_launch(
    existing: Option<Vec<PersistedQuickAccessPin>>,
    imported: Vec<PersistedQuickAccessPin>,
) -> Vec<PersistedQuickAccessPin> {
    existing.unwrap_or(imported)
}

#[cfg(test)]
mod tests {
    use explorer_model::LocationDescriptor;

    use super::*;

    fn pin(name: &str, path: &str, order: u32) -> PersistedQuickAccessPin {
        PersistedQuickAccessPin {
            location: LocationDescriptor::file_system(path),
            display_name: name.to_owned(),
            order,
        }
    }

    #[test]
    fn first_launch_without_session_imports_explorer_pins() {
        let imported = vec![
            pin("Projects", r"D:\Projects", 0),
            pin("Downloads", r"C:\Users\fixture\Downloads", 1),
        ];
        assert_eq!(
            seed_quick_access_for_first_launch(None, imported.clone()),
            imported
        );
    }

    #[test]
    fn existing_session_keeps_saved_pins_even_when_empty() {
        let imported = vec![pin("Projects", r"D:\Projects", 0)];
        assert!(seed_quick_access_for_first_launch(Some(Vec::new()), imported).is_empty());
    }

    #[test]
    fn existing_session_does_not_replace_user_pins() {
        let existing = vec![pin("Mine", r"D:\Mine", 0)];
        let imported = vec![pin("Projects", r"D:\Projects", 0)];
        assert_eq!(
            seed_quick_access_for_first_launch(Some(existing.clone()), imported),
            existing
        );
    }
}
