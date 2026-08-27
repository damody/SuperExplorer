//! Typed, ordered bookmark tree owned by the application session.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::LocationDescriptor;

pub type BookmarkId = Uuid;
pub type BookmarkFolderId = Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BookmarkTarget {
    Folder { location: LocationDescriptor },
    File { location: LocationDescriptor },
    FolderPath { path: String },
    FilePath { path: String },
    LuaScript { source: String },
}

impl BookmarkTarget {
    pub fn editable_payload(&self) -> String {
        match self {
            Self::Folder { location } | Self::File { location } => location.editable_text(),
            Self::FolderPath { path } | Self::FilePath { path } => path.clone(),
            Self::LuaScript { source } => source.clone(),
        }
    }

    pub fn with_editable_payload(&self, payload: String) -> Self {
        match self {
            Self::Folder { .. } | Self::FolderPath { .. } => Self::FolderPath { path: payload },
            Self::File { .. } | Self::FilePath { .. } => Self::FilePath { path: payload },
            Self::LuaScript { .. } => Self::LuaScript { source: payload },
        }
    }

    pub const fn is_folder(&self) -> bool {
        matches!(self, Self::Folder { .. } | Self::FolderPath { .. })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bookmark {
    pub id: BookmarkId,
    pub name: String,
    pub order: u32,
    #[serde(default)]
    pub parent_id: Option<BookmarkFolderId>,
    pub target: BookmarkTarget,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookmarkFolder {
    pub id: BookmarkFolderId,
    pub name: String,
    pub order: u32,
    #[serde(default)]
    pub parent_id: Option<BookmarkFolderId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Bookmarks {
    folders: Vec<BookmarkFolder>,
    entries: Vec<Bookmark>,
    legacy_encoding: bool,
}

#[derive(Serialize)]
struct TreeRef<'a> {
    version: u8,
    folders: &'a [BookmarkFolder],
    entries: &'a [Bookmark],
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Wire {
    Legacy(Vec<Bookmark>),
    Tree {
        #[serde(default)]
        folders: Vec<BookmarkFolder>,
        #[serde(default)]
        entries: Vec<Bookmark>,
    },
}

impl Serialize for Bookmarks {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.legacy_encoding {
            return self.entries.serialize(serializer);
        }
        TreeRef {
            version: 2,
            folders: &self.folders,
            entries: &self.entries,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Bookmarks {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut value = match Wire::deserialize(deserializer)? {
            Wire::Legacy(mut entries) => {
                for entry in &mut entries {
                    entry.parent_id = None;
                }
                Self {
                    folders: Vec::new(),
                    entries,
                    legacy_encoding: true,
                }
            }
            Wire::Tree { folders, entries } => Self {
                folders,
                entries,
                legacy_encoding: false,
            },
        };
        value.repair_tree();
        Ok(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookmarkMutation {
    previous: Bookmarks,
    changed: bool,
}

impl BookmarkMutation {
    fn new(previous: Bookmarks, changed: bool) -> Self {
        Self { previous, changed }
    }

    pub const fn changed(&self) -> bool {
        self.changed
    }
}

impl Bookmarks {
    pub fn entries(&self) -> &[Bookmark] {
        &self.entries
    }

    pub fn folders(&self) -> &[BookmarkFolder] {
        &self.folders
    }

    pub(crate) const fn uses_legacy_encoding(&self) -> bool {
        self.legacy_encoding
    }

    pub(crate) fn upgrade_encoding(&mut self) {
        self.legacy_encoding = false;
    }

    pub fn root_entries(&self) -> impl Iterator<Item = &Bookmark> {
        self.child_entries(None)
    }

    pub fn child_entries(
        &self,
        parent_id: Option<BookmarkFolderId>,
    ) -> impl Iterator<Item = &Bookmark> {
        self.entries
            .iter()
            .filter(move |item| item.parent_id == parent_id)
    }

    pub fn child_folders(
        &self,
        parent_id: Option<BookmarkFolderId>,
    ) -> impl Iterator<Item = &BookmarkFolder> {
        self.folders
            .iter()
            .filter(move |item| item.parent_id == parent_id)
    }

    pub fn folder(&self, id: BookmarkFolderId) -> Option<&BookmarkFolder> {
        self.folders.iter().find(|folder| folder.id == id)
    }

    pub fn id_for_target(&self, target: &BookmarkTarget) -> Option<BookmarkId> {
        self.entries
            .iter()
            .find(|item| &item.target == target)
            .map(|item| item.id)
    }

    pub fn replace(&mut self, mut entries: Vec<Bookmark>) {
        for entry in &mut entries {
            entry.parent_id = None;
        }
        self.folders.clear();
        self.entries = entries;
        self.legacy_encoding = false;
        self.normalize_orders();
    }

    pub fn begin_add(&mut self, name: String, target: BookmarkTarget) -> BookmarkMutation {
        self.begin_add_to(name, target, None)
    }

    pub fn begin_add_to(
        &mut self,
        name: String,
        target: BookmarkTarget,
        parent_id: Option<BookmarkFolderId>,
    ) -> BookmarkMutation {
        let previous = self.clone();
        if !self.valid_parent(parent_id) {
            return BookmarkMutation::new(previous, false);
        }
        self.entries.push(Bookmark {
            id: Uuid::new_v4(),
            name,
            order: self.next_order(parent_id),
            parent_id,
            target,
        });
        self.legacy_encoding = false;
        BookmarkMutation::new(previous, true)
    }

    pub fn begin_add_folder(
        &mut self,
        name: String,
        parent_id: Option<BookmarkFolderId>,
    ) -> BookmarkMutation {
        let previous = self.clone();
        if name.trim().is_empty() || !self.valid_parent(parent_id) {
            return BookmarkMutation::new(previous, false);
        }
        self.folders.push(BookmarkFolder {
            id: Uuid::new_v4(),
            name,
            order: self.next_order(parent_id),
            parent_id,
        });
        self.legacy_encoding = false;
        BookmarkMutation::new(previous, true)
    }

    pub fn begin_rename_folder(&mut self, id: BookmarkFolderId, name: String) -> BookmarkMutation {
        let previous = self.clone();
        let changed = !name.trim().is_empty()
            && self
                .folders
                .iter_mut()
                .find(|item| item.id == id)
                .is_some_and(|item| {
                    if item.name == name {
                        false
                    } else {
                        item.name = name;
                        true
                    }
                });
        if changed {
            self.legacy_encoding = false;
        }
        BookmarkMutation::new(previous, changed)
    }

    pub fn begin_update(
        &mut self,
        id: BookmarkId,
        name: String,
        target: BookmarkTarget,
    ) -> BookmarkMutation {
        let parent_id = self
            .entries
            .iter()
            .find(|item| item.id == id)
            .and_then(|item| item.parent_id);
        self.begin_update_in(id, name, target, parent_id)
    }

    pub fn begin_update_in(
        &mut self,
        id: BookmarkId,
        name: String,
        target: BookmarkTarget,
        parent_id: Option<BookmarkFolderId>,
    ) -> BookmarkMutation {
        let previous = self.clone();
        if !self.valid_parent(parent_id) {
            return BookmarkMutation::new(previous, false);
        }
        let changed = self
            .entries
            .iter_mut()
            .find(|item| item.id == id)
            .is_some_and(|item| {
                if item.name == name && item.target == target && item.parent_id == parent_id {
                    false
                } else {
                    item.name = name;
                    item.target = target;
                    item.parent_id = parent_id;
                    true
                }
            });
        if changed {
            self.legacy_encoding = false;
            self.normalize_orders();
        }
        BookmarkMutation::new(previous, changed)
    }

    pub fn begin_remove(&mut self, id: BookmarkId) -> BookmarkMutation {
        let previous = self.clone();
        let before = self.entries.len();
        self.entries.retain(|item| item.id != id);
        let changed = before != self.entries.len();
        if changed {
            self.legacy_encoding = false;
            self.normalize_orders();
        }
        BookmarkMutation::new(previous, changed)
    }

    pub fn descendant_count(&self, id: BookmarkFolderId) -> usize {
        let ids = self.descendant_ids(id);
        ids.len().saturating_sub(1)
            + self
                .entries
                .iter()
                .filter(|item| item.parent_id.is_some_and(|p| ids.contains(&p)))
                .count()
    }

    pub fn begin_remove_folder(
        &mut self,
        id: BookmarkFolderId,
        allow_non_empty: bool,
    ) -> BookmarkMutation {
        let previous = self.clone();
        if self.folder(id).is_none() || (!allow_non_empty && self.descendant_count(id) != 0) {
            return BookmarkMutation::new(previous, false);
        }
        let ids = self.descendant_ids(id);
        self.folders.retain(|item| !ids.contains(&item.id));
        self.entries
            .retain(|item| !item.parent_id.is_some_and(|p| ids.contains(&p)));
        self.normalize_orders();
        self.legacy_encoding = false;
        BookmarkMutation::new(previous, true)
    }

    pub fn begin_reorder(&mut self, id: BookmarkId, destination: usize) -> BookmarkMutation {
        let previous = self.clone();
        let Some(parent) = self
            .entries
            .iter()
            .find(|item| item.id == id)
            .map(|item| item.parent_id)
        else {
            return BookmarkMutation::new(previous, false);
        };
        let mut siblings = self
            .child_entries(parent)
            .map(|item| item.id)
            .collect::<Vec<_>>();
        let Some(source) = siblings.iter().position(|candidate| *candidate == id) else {
            return BookmarkMutation::new(previous, false);
        };
        if source == destination || destination >= siblings.len() {
            return BookmarkMutation::new(previous, false);
        }
        let moved = siblings.remove(source);
        siblings.insert(destination, moved);
        for (order, sibling) in siblings.into_iter().enumerate() {
            if let Some(item) = self.entries.iter_mut().find(|item| item.id == sibling) {
                item.order = u32::try_from(order).unwrap_or(u32::MAX);
            }
        }
        self.normalize_orders();
        self.legacy_encoding = false;
        BookmarkMutation::new(previous, true)
    }

    pub fn begin_move_to_folder(
        &mut self,
        id: BookmarkId,
        parent_id: Option<BookmarkFolderId>,
    ) -> BookmarkMutation {
        let previous = self.clone();
        if !self.valid_parent(parent_id) {
            return BookmarkMutation::new(previous, false);
        }
        let next_order = self.next_order(parent_id);
        let changed = self
            .entries
            .iter_mut()
            .find(|item| item.id == id)
            .is_some_and(|item| {
                if item.parent_id == parent_id {
                    false
                } else {
                    item.parent_id = parent_id;
                    item.order = next_order;
                    true
                }
            });
        if changed {
            self.normalize_orders();
            self.legacy_encoding = false;
        }
        BookmarkMutation::new(previous, changed)
    }

    pub fn rollback(&mut self, mutation: BookmarkMutation) {
        if mutation.changed {
            *self = mutation.previous;
        }
    }

    fn valid_parent(&self, id: Option<BookmarkFolderId>) -> bool {
        id.is_none_or(|id| self.folder(id).is_some())
    }

    fn next_order(&self, parent: Option<BookmarkFolderId>) -> u32 {
        u32::try_from(self.child_entries(parent).count() + self.child_folders(parent).count())
            .unwrap_or(u32::MAX)
    }

    fn descendant_ids(&self, id: BookmarkFolderId) -> HashSet<BookmarkFolderId> {
        let mut ids = HashSet::from([id]);
        loop {
            let before = ids.len();
            for folder in &self.folders {
                if folder.parent_id.is_some_and(|parent| ids.contains(&parent)) {
                    ids.insert(folder.id);
                }
            }
            if before == ids.len() {
                return ids;
            }
        }
    }

    fn repair_tree(&mut self) {
        let mut all_ids = HashSet::new();
        self.folders.retain(|item| all_ids.insert(item.id));
        self.entries.retain(|item| all_ids.insert(item.id));
        let folder_ids = self
            .folders
            .iter()
            .map(|item| item.id)
            .collect::<HashSet<_>>();
        for folder in &mut self.folders {
            if folder
                .parent_id
                .is_some_and(|parent| parent == folder.id || !folder_ids.contains(&parent))
            {
                folder.parent_id = None;
            }
        }
        let parents = self
            .folders
            .iter()
            .map(|item| (item.id, item.parent_id))
            .collect::<HashMap<_, _>>();
        for folder in &mut self.folders {
            let mut seen = HashSet::from([folder.id]);
            let mut cursor = folder.parent_id;
            while let Some(parent) = cursor {
                if !seen.insert(parent) {
                    folder.parent_id = None;
                    break;
                }
                cursor = parents.get(&parent).copied().flatten();
            }
        }
        let valid = self
            .folders
            .iter()
            .map(|item| item.id)
            .collect::<HashSet<_>>();
        for entry in &mut self.entries {
            if entry
                .parent_id
                .is_some_and(|parent| !valid.contains(&parent))
            {
                entry.parent_id = None;
            }
        }
        self.normalize_orders();
    }

    fn normalize_orders(&mut self) {
        let parents = self
            .folders
            .iter()
            .map(|item| item.parent_id)
            .chain(self.entries.iter().map(|item| item.parent_id))
            .collect::<HashSet<_>>();
        for parent in parents {
            let mut items = self
                .folders
                .iter()
                .filter(|item| item.parent_id == parent)
                .map(|item| (item.order, item.id, true))
                .chain(
                    self.entries
                        .iter()
                        .filter(|item| item.parent_id == parent)
                        .map(|item| (item.order, item.id, false)),
                )
                .collect::<Vec<_>>();
            items.sort_by_key(|item| (item.0, item.1));
            for (order, (_, id, folder)) in items.into_iter().enumerate() {
                if folder {
                    if let Some(item) = self.folders.iter_mut().find(|item| item.id == id) {
                        item.order = u32::try_from(order).unwrap_or(u32::MAX);
                    }
                } else if let Some(item) = self.entries.iter_mut().find(|item| item.id == id) {
                    item.order = u32::try_from(order).unwrap_or(u32::MAX);
                }
            }
        }
        self.folders
            .sort_by_key(|item| (item.parent_id, item.order, item.id));
        self.entries
            .sort_by_key(|item| (item.parent_id, item.order, item.id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bookmark_moves_between_root_and_folder_with_rollback() {
        let mut value = Bookmarks::default();
        value.begin_add_folder("Folder".into(), None);
        let folder = value.folders()[0].id;
        value.begin_add(
            "Entry".into(),
            BookmarkTarget::LuaScript {
                source: "return 1".into(),
            },
        );
        let entry = value.entries()[0].id;
        let mutation = value.begin_move_to_folder(entry, Some(folder));
        assert!(mutation.changed());
        assert_eq!(value.entries()[0].parent_id, Some(folder));
        assert!(!value.begin_move_to_folder(entry, Some(folder)).changed());
        value.rollback(mutation);
        assert_eq!(value.entries()[0].parent_id, None);
        assert!(
            !value
                .begin_move_to_folder(entry, Some(Uuid::new_v4()))
                .changed()
        );
    }

    #[test]
    fn tree_crud_and_rollback_are_recursive() {
        let mut value = Bookmarks::default();
        value.begin_add_folder("Work".into(), None);
        let folder = value.folders()[0].id;
        value.begin_add_to(
            "Lua".into(),
            BookmarkTarget::LuaScript {
                source: "return 1".into(),
            },
            Some(folder),
        );
        value.begin_add_folder("Nested".into(), Some(folder));
        assert_eq!(value.descendant_count(folder), 2);
        assert!(!value.begin_remove_folder(folder, false).changed());
        let mutation = value.begin_remove_folder(folder, true);
        assert!(mutation.changed());
        value.rollback(mutation);
        assert_eq!(value.folders().len(), 2);
        assert_eq!(value.entries().len(), 1);
    }

    #[test]
    fn legacy_array_upgrades_losslessly() {
        let id = Uuid::new_v4();
        let json = format!(
            r#"[{{"id":"{id}","name":"Legacy","order":0,"target":{{"kind":"lua_script","source":"return 1"}}}}]"#
        );
        let mut value: Bookmarks = serde_json::from_str(&json).expect("legacy decode");
        assert_eq!(value.entries()[0].id, id);
        assert_eq!(value.entries()[0].parent_id, None);
        assert!(value.uses_legacy_encoding());
        value.upgrade_encoding();
        assert!(
            serde_json::to_string(&value)
                .expect("tree encode")
                .contains("\"version\":2")
        );
    }

    #[test]
    fn invalid_parents_and_cycles_recover_at_root() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let json = format!(
            r#"{{"version":2,"folders":[{{"id":"{a}","name":"A","order":2,"parent_id":"{b}"}},{{"id":"{b}","name":"B","order":1,"parent_id":"{a}"}}],"entries":[{{"id":"{}","name":"Orphan","order":4,"parent_id":"{}","target":{{"kind":"lua_script","source":"return 1"}}}}]}}"#,
            Uuid::new_v4(),
            Uuid::new_v4()
        );
        let value: Bookmarks = serde_json::from_str(&json).expect("repair tree");
        assert!(value.folders().iter().any(|item| item.parent_id.is_none()));
        assert_eq!(value.entries()[0].parent_id, None);
    }

    #[test]
    fn update_reorder_and_lookup_keep_typed_targets() {
        let mut value = Bookmarks::default();
        let target = BookmarkTarget::Folder {
            location: LocationDescriptor::file_system(r"C:\fixture"),
        };
        value.begin_add("Fixture".into(), target.clone());
        value.begin_add(
            "Other".into(),
            BookmarkTarget::LuaScript {
                source: "return 1".into(),
            },
        );
        let id = value.entries()[0].id;
        assert_eq!(value.id_for_target(&target), Some(id));
        assert!(value.begin_reorder(id, 1).changed());
        assert!(value.begin_update(id, "Renamed".into(), target).changed());
    }

    #[test]
    fn remote_folder_bookmarks_round_trip_public_authority_without_secrets() {
        let mut value = Bookmarks::default();
        for address in [
            "adb://emulator-5554/sdcard/Android",
            "sftp://production/root/uploads",
        ] {
            let location = crate::RemoteAddress::parse(address)
                .unwrap()
                .to_deterministic_location(1)
                .unwrap();
            value.begin_add(address.to_owned(), BookmarkTarget::Folder { location });
        }
        let encoded = serde_json::to_string(&value).unwrap();
        assert!(encoded.contains("emulator-5554"));
        assert!(encoded.contains("production"));
        assert!(!encoded.contains("password"));
        assert!(!encoded.contains("45.32.49.125"));
        let decoded: Bookmarks = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.entries().len(), 2);
    }

    #[test]
    fn raw_path_targets_round_trip_exact_text_without_validation() {
        let mut value = Bookmarks::default();
        for (name, target) in [
            (
                "Malformed",
                BookmarkTarget::FolderPath {
                    path: r#"?:\\not\a\valid\path<>"#.to_owned(),
                },
            ),
            (
                "Offline",
                BookmarkTarget::FilePath {
                    path: r#"sftp://offline host/future/file.txt"#.to_owned(),
                },
            ),
            (
                "Virtual",
                BookmarkTarget::FolderPath {
                    path: "virtual-provider://missing/container".to_owned(),
                },
            ),
        ] {
            value.begin_add(name.to_owned(), target);
        }
        let encoded = serde_json::to_string(&value).expect("encode raw targets");
        let decoded: Bookmarks = serde_json::from_str(&encoded).expect("decode raw targets");
        assert_eq!(decoded, value);
        assert_eq!(
            decoded.entries()[0].target.editable_payload(),
            r#"?:\\not\a\valid\path<>"#
        );
        assert_eq!(
            decoded.entries()[1].target.editable_payload(),
            "sftp://offline host/future/file.txt"
        );
    }

    #[test]
    fn structured_targets_remain_editable_without_changing_legacy_encoding() {
        let target = BookmarkTarget::Folder {
            location: LocationDescriptor::file_system(r"C:\legacy\missing"),
        };
        assert_eq!(target.editable_payload(), r"C:\legacy\missing");
        assert_eq!(
            target.with_editable_payload("shell:FutureFolder".to_owned()),
            BookmarkTarget::FolderPath {
                path: "shell:FutureFolder".to_owned()
            }
        );
    }
}
