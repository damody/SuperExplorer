//! Typed, ordered bookmark tree owned by the application session.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::LocationDescriptor;

fn bookmark_now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

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
    Separator,
}

impl BookmarkTarget {
    pub fn editable_payload(&self) -> String {
        match self {
            Self::Folder { location } | Self::File { location } => location.editable_text(),
            Self::FolderPath { path } | Self::FilePath { path } => path.clone(),
            Self::LuaScript { source } => source.clone(),
            Self::Separator => String::new(),
        }
    }

    pub fn with_editable_payload(&self, payload: String) -> Self {
        match self {
            Self::Folder { .. } | Self::FolderPath { .. } => Self::FolderPath { path: payload },
            Self::File { .. } | Self::FilePath { .. } => Self::FilePath { path: payload },
            Self::LuaScript { .. } => Self::LuaScript { source: payload },
            Self::Separator => Self::Separator,
        }
    }

    pub const fn is_folder(&self) -> bool {
        matches!(self, Self::Folder { .. } | Self::FolderPath { .. })
    }

    fn identifies_same_item(&self, other: &Self) -> bool {
        if self == other {
            return true;
        }
        let same_kind = matches!(
            (self, other),
            (
                Self::Folder { .. } | Self::FolderPath { .. },
                Self::Folder { .. } | Self::FolderPath { .. }
            ) | (
                Self::File { .. } | Self::FilePath { .. },
                Self::File { .. } | Self::FilePath { .. }
            )
        );
        same_kind && self.editable_payload() == other.editable_payload()
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
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub added_epoch_seconds: u64,
    #[serde(default)]
    pub modified_epoch_seconds: u64,
    #[serde(default)]
    pub visited_epoch_seconds: u64,
    #[serde(default)]
    pub visit_count: u32,
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
            .find(|item| item.target.identifies_same_item(target))
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
        let now = bookmark_now_epoch();
        self.entries.push(Bookmark {
            id: Uuid::new_v4(),
            name,
            order: self.next_order(parent_id),
            parent_id,
            target,
            tags: String::new(),
            added_epoch_seconds: now,
            modified_epoch_seconds: now,
            visited_epoch_seconds: 0,
            visit_count: 0,
        });
        self.legacy_encoding = false;
        BookmarkMutation::new(previous, true)
    }

    pub fn begin_add_separator(&mut self, parent_id: Option<BookmarkFolderId>) -> BookmarkMutation {
        self.begin_add_to(String::new(), BookmarkTarget::Separator, parent_id)
    }

    pub fn record_visit(&mut self, id: BookmarkId) -> BookmarkMutation {
        let previous = self.clone();
        let now = bookmark_now_epoch();
        let changed = self
            .entries
            .iter_mut()
            .find(|item| item.id == id)
            .is_some_and(|item| {
                item.visit_count = item.visit_count.saturating_add(1);
                item.visited_epoch_seconds = now;
                true
            });
        if changed {
            self.legacy_encoding = false;
        }
        BookmarkMutation::new(previous, changed)
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
                    item.modified_epoch_seconds = bookmark_now_epoch();
                    true
                }
            });
        if changed {
            self.legacy_encoding = false;
            self.normalize_orders();
        }
        BookmarkMutation::new(previous, changed)
    }

    pub fn set_entry_tags(&mut self, id: BookmarkId, tags: String) -> bool {
        self.entries
            .iter_mut()
            .find(|item| item.id == id)
            .is_some_and(|item| {
                if item.tags == tags {
                    false
                } else {
                    item.tags = tags;
                    item.modified_epoch_seconds = bookmark_now_epoch();
                    true
                }
            })
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

    pub fn to_netscape_html(&self) -> String {
        let mut out = String::from(
            "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n\
             <META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n\
             <TITLE>Bookmarks</TITLE>\n\
             <H1>Bookmarks</H1>\n\
             <DL><p>\n",
        );
        write_netscape_children(&mut out, self, None, 1);
        out.push_str("</DL><p>\n");
        out
    }

    pub fn from_netscape_html(html: &str) -> Result<Self, String> {
        if !html.to_ascii_uppercase().contains("NETSCAPE-BOOKMARK-FILE")
            && !html.to_ascii_lowercase().contains("<dt>")
        {
            return Err("not a Netscape bookmark file".to_owned());
        }
        let mut bookmarks = Self::default();
        parse_netscape_html(&mut bookmarks, html, None)?;
        Ok(bookmarks)
    }

    pub fn from_chromium_json(json: &str) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|error| error.to_string())?;
        let roots = value
            .get("roots")
            .ok_or_else(|| "missing Chromium roots".to_owned())?;
        let mut bookmarks = Self::default();
        for key in ["bookmark_bar", "other", "synced"] {
            if let Some(node) = roots.get(key) {
                import_chromium_node(&mut bookmarks, node, None)?;
            }
        }
        if bookmarks.entries.is_empty() && bookmarks.folders.is_empty() {
            return Err("no Chromium bookmarks found".to_owned());
        }
        Ok(bookmarks)
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn write_netscape_children(
    out: &mut String,
    bookmarks: &Bookmarks,
    parent: Option<BookmarkFolderId>,
    depth: usize,
) {
    let indent = "    ".repeat(depth);
    let mut folders: Vec<_> = bookmarks.child_folders(parent).collect();
    folders.sort_by_key(|folder| folder.order);
    let mut entries: Vec<_> = bookmarks.child_entries(parent).collect();
    entries.sort_by_key(|entry| entry.order);
    let mut items: Vec<(u32, bool, usize)> = folders
        .iter()
        .enumerate()
        .map(|(index, folder)| (folder.order, true, index))
        .chain(
            entries
                .iter()
                .enumerate()
                .map(|(index, entry)| (entry.order, false, index)),
        )
        .collect();
    items.sort_by_key(|item| (item.0, item.1, item.2));
    for (_, is_folder, index) in items {
        if is_folder {
            let folder = folders[index];
            out.push_str(&indent);
            out.push_str("<DT><H3>");
            out.push_str(&html_escape(&folder.name));
            out.push_str("</H3>\n");
            out.push_str(&indent);
            out.push_str("<DL><p>\n");
            write_netscape_children(out, bookmarks, Some(folder.id), depth + 1);
            out.push_str(&indent);
            out.push_str("</DL><p>\n");
        } else {
            let entry = entries[index];
            out.push_str(&indent);
            if matches!(entry.target, BookmarkTarget::Separator) {
                out.push_str("<DT><HR>\n");
            } else {
                out.push_str("<DT><A HREF=\"");
                out.push_str(&html_escape(&entry.target.editable_payload()));
                out.push_str("\">");
                out.push_str(&html_escape(&entry.name));
                out.push_str("</A>\n");
            }
        }
    }
}

fn parse_netscape_html(
    bookmarks: &mut Bookmarks,
    html: &str,
    parent: Option<BookmarkFolderId>,
) -> Result<(), String> {
    let mut i = 0;
    let upper = html.to_ascii_uppercase();
    while i < html.len() {
        let remaining = &upper[i..];
        let href_pos = remaining.find("HREF=\"");
        let h3_pos = remaining.find("<H3");
        let hr_pos = remaining.find("<HR");
        let dl_pos = remaining.find("<DL");
        let next = [href_pos, h3_pos, hr_pos, dl_pos]
            .into_iter()
            .flatten()
            .min();
        let Some(rel) = next else {
            break;
        };
        if Some(rel) == dl_pos && i > 0 {
            let inner = extract_dl_after(&html[i + rel..]).unwrap_or_default();
            parse_netscape_html(bookmarks, &inner, parent)?;
            i += rel
                + skip_first_dl(&html[i + rel..])
                    .map(|after| html[i + rel..].len() - after.len())
                    .unwrap_or(4);
            continue;
        }
        if Some(rel) == href_pos {
            let raw = &html[i + rel + 6..];
            if let Some(end) = raw.find('"') {
                let href = html_unescape(&raw[..end]);
                let after = &raw[end + 1..];
                if let Some(gt) = after.find('>') {
                    let name_src = &after[gt + 1..];
                    if let Some(close) = name_src.find("</A>").or_else(|| name_src.find("</a>")) {
                        let name = strip_tags(&name_src[..close]);
                        let _ = bookmarks.begin_add_to(
                            name,
                            BookmarkTarget::FolderPath { path: href },
                            parent,
                        );
                        i += rel + 6 + end + 1 + gt + 1 + close + 4;
                        continue;
                    }
                }
            }
            i += rel + 6;
            continue;
        }
        if Some(rel) == hr_pos {
            let _ = bookmarks.begin_add_separator(parent);
            i += rel + 3;
            continue;
        }
        if Some(rel) == h3_pos {
            if let Some(name) = extract_between(&html[i + rel..], "<H3", "</H3>")
                .or_else(|| extract_between(&html[i + rel..], "<h3", "</h3>"))
            {
                let folder_name = strip_tags(&name);
                let _ = bookmarks.begin_add_folder(folder_name.clone(), parent);
                let folder_id = bookmarks
                    .folders()
                    .iter()
                    .rev()
                    .find(|folder| folder.name == folder_name && folder.parent_id == parent)
                    .map(|folder| folder.id);
                let after_h3 = html[i + rel..]
                    .find("</H3>")
                    .or_else(|| html[i + rel..].find("</h3>"))
                    .map(|value| i + rel + value + 5)
                    .unwrap_or(i + rel + 4);
                let search = html.get(after_h3..).unwrap_or("");
                if let Some(inner) = extract_dl_after(search) {
                    parse_netscape_html(bookmarks, &inner, folder_id)?;
                    i = after_h3
                        + skip_first_dl(search)
                            .map(|after| search.len() - after.len())
                            .unwrap_or(0);
                    continue;
                }
                i = after_h3;
                continue;
            }
        }
        i += rel + 1;
    }
    Ok(())
}

fn skip_first_dl(input: &str) -> Option<&str> {
    let start = input.to_ascii_uppercase().find("<DL")?;
    let after = input.get(start..)?;
    let mut depth = 0;
    let mut i = 0;
    while i + 4 < after.len() {
        let slice = after[i..].to_ascii_uppercase();
        if slice.starts_with("<DL") {
            depth += 1;
            i += 3;
        } else if slice.starts_with("</DL") {
            depth -= 1;
            if depth == 0 {
                let close = after[i..].find('>').unwrap_or(4);
                return after.get(i + close + 1..);
            }
            i += 4;
        } else {
            i += 1;
        }
    }
    None
}

fn extract_between(input: &str, start: &str, end: &str) -> Option<String> {
    let from = input.find(start)? + start.len();
    let after = input.get(from..)?;
    let close = after.find('>')? + 1;
    let body = after.get(close..)?;
    let to = body.find(end)?;
    Some(body[..to].to_owned())
}

fn extract_dl_after(input: &str) -> Option<String> {
    let upper = input.to_ascii_uppercase();
    let start = upper.find("<DL")?;
    let after = input.get(start..)?;
    let inner_start = after.find('>')? + 1;
    let mut depth = 1;
    let bytes = after.as_bytes();
    let mut i = inner_start;
    while i + 4 < after.len() {
        if after[i..].to_ascii_uppercase().starts_with("<DL") {
            depth += 1;
            i += 3;
        } else if after[i..].to_ascii_uppercase().starts_with("</DL") {
            depth -= 1;
            if depth == 0 {
                return Some(after[inner_start..i].to_owned());
            }
            i += 4;
        } else {
            i += 1;
        }
    }
    let _ = bytes;
    None
}

fn strip_tags(value: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for ch in value.chars() {
        match ch {
            '<' => skipping = true,
            '>' => skipping = false,
            _ if !skipping => out.push(ch),
            _ => {}
        }
    }
    html_unescape(out.trim())
}

fn html_unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn import_chromium_node(
    bookmarks: &mut Bookmarks,
    node: &serde_json::Value,
    parent: Option<BookmarkFolderId>,
) -> Result<(), String> {
    let kind = node
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("folder");
    match kind {
        "url" => {
            let name = node
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Bookmark")
                .to_owned();
            let url = node
                .get("url")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if !url.is_empty() {
                let _ =
                    bookmarks.begin_add_to(name, BookmarkTarget::FolderPath { path: url }, parent);
            }
        }
        _ => {
            let name = node
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Folder")
                .to_owned();
            let folder_id = if parent.is_none()
                && matches!(
                    name.as_str(),
                    "Bookmarks bar" | "Other bookmarks" | "Mobile bookmarks" | ""
                ) {
                parent
            } else if name.is_empty() {
                parent
            } else {
                let _ = bookmarks.begin_add_folder(name.clone(), parent);
                bookmarks
                    .child_folders(parent)
                    .find(|folder| folder.name == name)
                    .map(|folder| folder.id)
                    .or(parent)
            };
            if let Some(children) = node.get("children").and_then(serde_json::Value::as_array) {
                for child in children {
                    import_chromium_node(bookmarks, child, folder_id)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netscape_html_round_trips_folders_bookmarks_and_separators() {
        let mut value = Bookmarks::default();
        value.begin_add(
            "portable".into(),
            BookmarkTarget::FolderPath {
                path: r"C:\portable".into(),
            },
        );
        value.begin_add_folder("super".into(), None);
        let folder = value.folders()[0].id;
        value.begin_add_separator(None);
        value.begin_add_to(
            "nested".into(),
            BookmarkTarget::FolderPath {
                path: r"D:\nested".into(),
            },
            Some(folder),
        );
        let html = value.to_netscape_html();
        assert!(html.contains("NETSCAPE-Bookmark-file-1"), "{html}");
        assert!(html.contains("<HR>"), "{html}");
        assert!(html.contains("C:\\portable"), "{html}");
        let decoded = Bookmarks::from_netscape_html(&html).expect("parse html");
        let names: Vec<_> = decoded
            .entries()
            .iter()
            .map(|item| (item.name.as_str(), item.target.editable_payload()))
            .collect();
        assert!(
            names.iter().any(|(name, _)| *name == "portable"),
            "html={html}\nparsed={names:?}"
        );
        assert!(decoded.folders().iter().any(|item| item.name == "super"));
        assert!(
            decoded
                .entries()
                .iter()
                .any(|item| matches!(item.target, BookmarkTarget::Separator))
        );
        assert!(
            decoded
                .entries()
                .iter()
                .any(|item| item.name == "nested" && item.parent_id.is_some())
        );
    }

    #[test]
    fn chromium_json_imports_url_nodes() {
        let json = r#"{
            "roots": {
                "bookmark_bar": {
                    "type": "folder",
                    "name": "Bookmarks bar",
                    "children": [
                        {"type": "url", "name": "Example", "url": "https://example.com"}
                    ]
                }
            }
        }"#;
        let imported = Bookmarks::from_chromium_json(json).expect("chromium json");
        assert_eq!(imported.entries()[0].name, "Example");
        assert_eq!(
            imported.entries()[0].target.editable_payload(),
            "https://example.com"
        );
    }

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
    fn lookup_matches_editable_folder_paths_inside_bookmark_folders() {
        let mut value = Bookmarks::default();
        value.begin_add_folder("Work".into(), None);
        let parent_id = value.folders()[0].id;
        value.begin_add_to(
            "Fixture".into(),
            BookmarkTarget::FolderPath {
                path: r"C:\fixture".to_owned(),
            },
            Some(parent_id),
        );
        let id = value.entries()[0].id;

        assert_eq!(
            value.id_for_target(&BookmarkTarget::Folder {
                location: LocationDescriptor::file_system(r"C:\fixture"),
            }),
            Some(id)
        );
        assert_eq!(
            value.id_for_target(&BookmarkTarget::File {
                location: LocationDescriptor::file_system(r"C:\fixture"),
            }),
            None,
            "folder and file bookmarks must remain distinct"
        );
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
