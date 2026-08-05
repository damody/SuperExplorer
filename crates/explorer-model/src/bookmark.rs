//! Typed, ordered bookmarks owned by the application session.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::LocationDescriptor;

pub type BookmarkId = Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BookmarkTarget {
    Folder { location: LocationDescriptor },
    File { location: LocationDescriptor },
    LuaScript { source: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bookmark {
    pub id: Uuid,
    pub name: String,
    pub order: u32,
    pub target: BookmarkTarget,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Bookmarks(Vec<Bookmark>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookmarkMutation {
    previous: Bookmarks,
    changed: bool,
}

impl Bookmarks {
    pub fn entries(&self) -> &[Bookmark] {
        &self.0
    }

    pub fn id_for_target(&self, target: &BookmarkTarget) -> Option<BookmarkId> {
        self.0
            .iter()
            .find(|entry| &entry.target == target)
            .map(|entry| entry.id)
    }

    pub fn replace(&mut self, entries: Vec<Bookmark>) {
        self.0 = entries;
        self.normalize_order();
    }

    pub fn begin_add(&mut self, name: String, target: BookmarkTarget) -> BookmarkMutation {
        let previous = self.clone();
        let order = u32::try_from(self.0.len()).unwrap_or(u32::MAX);
        self.0.push(Bookmark {
            id: Uuid::new_v4(),
            name,
            order,
            target,
        });
        BookmarkMutation {
            previous,
            changed: true,
        }
    }

    pub fn begin_update(
        &mut self,
        id: Uuid,
        name: String,
        target: BookmarkTarget,
    ) -> BookmarkMutation {
        let previous = self.clone();
        let changed = self
            .0
            .iter_mut()
            .find(|entry| entry.id == id)
            .is_some_and(|entry| {
                if entry.name == name && entry.target == target {
                    return false;
                }
                entry.name = name;
                entry.target = target;
                true
            });
        BookmarkMutation { previous, changed }
    }

    pub fn begin_remove(&mut self, id: Uuid) -> BookmarkMutation {
        let previous = self.clone();
        let changed = self
            .0
            .iter()
            .position(|entry| entry.id == id)
            .is_some_and(|index| {
                self.0.remove(index);
                self.normalize_order();
                true
            });
        BookmarkMutation { previous, changed }
    }

    pub fn begin_reorder(&mut self, id: Uuid, destination: usize) -> BookmarkMutation {
        let previous = self.clone();
        let changed = destination < self.0.len()
            && self
                .0
                .iter()
                .position(|entry| entry.id == id)
                .is_some_and(|source| {
                    if source == destination {
                        return false;
                    }
                    let entry = self.0.remove(source);
                    self.0.insert(destination, entry);
                    self.normalize_order();
                    true
                });
        BookmarkMutation { previous, changed }
    }

    pub fn rollback(&mut self, mutation: BookmarkMutation) {
        if mutation.changed {
            *self = mutation.previous;
        }
    }

    fn normalize_order(&mut self) {
        for (index, entry) in self.0.iter_mut().enumerate() {
            entry.order = u32::try_from(index).unwrap_or(u32::MAX);
        }
    }
}

impl BookmarkMutation {
    pub const fn changed(&self) -> bool {
        self.changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutations_normalize_order_and_rollback() {
        let mut bookmarks = Bookmarks::default();
        let first = bookmarks.begin_add(
            "A".into(),
            BookmarkTarget::LuaScript {
                source: "return 1".into(),
            },
        );
        assert!(first.changed());
        let id = bookmarks.entries()[0].id;
        bookmarks.begin_add(
            "B".into(),
            BookmarkTarget::Folder {
                location: LocationDescriptor::file_system(r"C:\\fixture"),
            },
        );
        let removal = bookmarks.begin_remove(id);
        assert_eq!(bookmarks.entries()[0].order, 0);
        bookmarks.rollback(removal);
        assert_eq!(bookmarks.entries().len(), 2);
    }

    #[test]
    fn update_and_json_round_trip_preserve_typed_payload() {
        let mut bookmarks = Bookmarks::default();
        bookmarks.begin_add(
            "Command".into(),
            BookmarkTarget::LuaScript {
                source: "assert(current_folder)".into(),
            },
        );
        let id = bookmarks.entries()[0].id;
        assert!(
            bookmarks
                .begin_update(
                    id,
                    "Folder".into(),
                    BookmarkTarget::Folder {
                        location: LocationDescriptor::file_system(r"C:\fixture"),
                    },
                )
                .changed()
        );

        let encoded = serde_json::to_string(&bookmarks).expect("serialize bookmarks");
        let decoded: Bookmarks = serde_json::from_str(&encoded).expect("deserialize bookmarks");
        assert_eq!(decoded, bookmarks);
        assert!(matches!(
            &decoded.entries()[0].target,
            BookmarkTarget::Folder { .. }
        ));
    }

    #[test]
    fn finds_existing_bookmark_by_typed_target() {
        let mut bookmarks = Bookmarks::default();
        let target = BookmarkTarget::Folder {
            location: LocationDescriptor::file_system(r"C:\fixture"),
        };
        bookmarks.begin_add("Fixture".into(), target.clone());

        assert_eq!(
            bookmarks.id_for_target(&target),
            Some(bookmarks.entries()[0].id)
        );
        assert_eq!(
            bookmarks.id_for_target(&BookmarkTarget::File {
                location: LocationDescriptor::file_system(r"C:\fixture"),
            }),
            None
        );
    }
}
