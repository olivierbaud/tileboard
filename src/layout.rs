//! The layout: the tiles a person built, and where each one sits.
//!
//! It is one small document, saved whole when edit mode is closed, so it lives
//! in a single JSON file: nothing to query, nothing to install, and backing it
//! up is copying a file.

use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::contract::{View, is_id};

/// The grid is this many columns wide. `x` and `w` count columns; `y` and `h` count rows.
pub const COLUMNS: u32 = 12;

const MAX_TILES: usize = 500;
const MAX_FIELDS: usize = 50;
const MAX_PARAMS: usize = 20;
const MAX_TEXT: usize = 200;
const MAX_ROWS: u32 = 10_000;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    /// Goes up by one with every save. A save must name the `rev` it started from.
    #[serde(default)]
    pub rev: u64,
    pub tiles: Vec<Tile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tile {
    pub id: String,
    /// The tile's title. Empty means "use the field's own label".
    #[serde(default)]
    pub label: String,
    pub view: View,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub fields: Vec<FieldRef>,
}

/// One data point on a tile: which app, which source (with which params), which field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldRef {
    pub app: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
    pub field: String,
}

impl Layout {
    /// Everything wrong with this layout, in words. Empty means it can be saved.
    ///
    /// Only the shape is checked. Whether the apps, sources and fields exist is
    /// not: an app can be down or change, and its tiles must survive that.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.tiles.len() > MAX_TILES {
            out.push(format!("there are {} tiles; the most is {MAX_TILES}", self.tiles.len()));
        }
        let mut ids = HashSet::new();
        for (i, tile) in self.tiles.iter().enumerate() {
            let at = format!("tile {}", i + 1);
            let id_ok = (1..=64).contains(&tile.id.len())
                && tile
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-');
            if !id_ok {
                out.push(format!("{at}: id must be 1 to 64 letters, digits, _ or -"));
            } else if !ids.insert(&tile.id) {
                out.push(format!("{at}: id {:?} is used twice", tile.id));
            }
            if tile.label.chars().count() > MAX_TEXT {
                out.push(format!("{at}: label is longer than {MAX_TEXT} characters"));
            }
            if tile.w == 0 || tile.h == 0 {
                out.push(format!("{at}: w and h must be 1 or more"));
            }
            if tile.x.saturating_add(tile.w) > COLUMNS {
                out.push(format!("{at}: x + w goes past the {COLUMNS} columns"));
            }
            if tile.y > MAX_ROWS || tile.h > MAX_ROWS {
                out.push(format!("{at}: y and h must be at most {MAX_ROWS}"));
            }
            if tile.fields.is_empty() || tile.fields.len() > MAX_FIELDS {
                out.push(format!("{at}: a tile has 1 to {MAX_FIELDS} fields"));
            }
            for (n, field) in tile.fields.iter().enumerate() {
                let at = format!("{at}, field {}", n + 1);
                if !is_id(&field.app) || !is_id(&field.source) || !is_id(&field.field) {
                    out.push(format!("{at}: app, source and field must be ids"));
                }
                if field.params.len() > MAX_PARAMS {
                    out.push(format!("{at}: there are more than {MAX_PARAMS} params"));
                }
                if field
                    .params
                    .iter()
                    .any(|(name, value)| !is_id(name) || value.chars().count() > MAX_TEXT)
                {
                    out.push(format!(
                        "{at}: a param needs an id as name and a value of at most {MAX_TEXT} characters"
                    ));
                }
            }
        }
        out
    }
}

/// Why a layout was not saved.
#[derive(Debug, PartialEq)]
pub enum SaveError {
    /// Someone else saved first. `current` is the `rev` to reload.
    Conflict {
        current: u64,
    },
    Invalid(Vec<String>),
    Io(String),
}

/// Holds the current layout and writes every save to its file.
pub struct LayoutStore {
    /// `None` keeps the layout in memory only.
    path: Option<PathBuf>,
    current: Mutex<Layout>,
}

impl LayoutStore {
    /// Opens the layout file. If there is no file yet, starts from `starter`.
    /// A file that cannot be read or is not a valid layout is an error: it is never overwritten.
    pub fn open(path: Option<PathBuf>, starter: Layout) -> Result<LayoutStore, String> {
        let current = match &path {
            Some(path) if path.exists() => {
                let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
                let layout: Layout =
                    serde_json::from_str(&text).map_err(|e| format!("{} is not a layout file: {e}", path.display()))?;
                let problems = layout.problems();
                if !problems.is_empty() {
                    return Err(format!(
                        "{} is not a valid layout: {}",
                        path.display(),
                        problems.join("; ")
                    ));
                }
                layout
            }
            _ => starter,
        };
        Ok(LayoutStore {
            path,
            current: Mutex::new(current),
        })
    }

    pub fn get(&self) -> Layout {
        self.lock().clone()
    }

    pub fn rev(&self) -> u64 {
        self.lock().rev
    }

    /// Saves `incoming` if it started from the current `rev`, and returns what was saved.
    pub fn save(&self, incoming: Layout) -> Result<Layout, SaveError> {
        let problems = incoming.problems();
        if !problems.is_empty() {
            return Err(SaveError::Invalid(problems));
        }
        // The lock is held while writing, so saves go one at a time.
        let mut current = self.lock();
        if incoming.rev != current.rev {
            return Err(SaveError::Conflict { current: current.rev });
        }
        let saved = Layout {
            rev: current.rev + 1,
            tiles: incoming.tiles,
        };
        if let Some(path) = &self.path {
            write_whole(path, &saved).map_err(|e| SaveError::Io(format!("cannot save {}: {e}", path.display())))?;
        }
        *current = saved.clone();
        Ok(saved)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Layout> {
        // Nothing here can leave the layout half-changed, so a poisoned lock is still good.
        self.current.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Writes the layout to a new file, then swaps it in: a crash never leaves half a layout.
fn write_whole(path: &std::path::Path, layout: &Layout) -> std::io::Result<()> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = serde_json::to_string_pretty(layout).map_err(std::io::Error::other)?;
    text.push('\n');

    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    let mut file = std::fs::File::create(&temp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(id: &str) -> Tile {
        Tile {
            id: id.to_string(),
            label: "CPU".to_string(),
            view: View::Number,
            x: 0,
            y: 0,
            w: 3,
            h: 2,
            fields: vec![FieldRef {
                app: "demo".to_string(),
                source: "server".to_string(),
                params: BTreeMap::from([("host".to_string(), "atlas".to_string())]),
                field: "cpu".to_string(),
            }],
        }
    }

    fn layout(rev: u64, ids: &[&str]) -> Layout {
        Layout {
            rev,
            tiles: ids.iter().map(|id| tile(id)).collect(),
        }
    }

    /// A fresh folder for one test, removed when the test ends.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!("tileboard-test-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn file(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_layout_is_written_the_way_the_plan_shows() {
        let json = serde_json::to_value(layout(3, &["t1"])).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "rev": 3,
                "tiles": [{
                    "id": "t1", "label": "CPU", "view": "number", "x": 0, "y": 0, "w": 3, "h": 2,
                    "fields": [{"app": "demo", "source": "server", "params": {"host": "atlas"}, "field": "cpu"}]
                }]
            })
        );
    }

    #[test]
    fn a_good_layout_has_no_problems() {
        assert_eq!(layout(0, &["t1", "t2", "Tile_3-b"]).problems(), Vec::<String>::new());
        assert_eq!(Layout::default().problems(), Vec::<String>::new());
    }

    #[test]
    fn layout_problems_are_found() {
        type Change = Box<dyn FnOnce(&mut Layout)>;
        let cases: Vec<(&str, Change)> = vec![
            ("id must be", Box::new(|l| l.tiles[0].id = "tile 1".into())),
            ("id must be", Box::new(|l| l.tiles[0].id = String::new())),
            (
                "id \"t1\" is used twice",
                Box::new(|l| l.tiles.push(l.tiles[0].clone())),
            ),
            ("label is longer", Box::new(|l| l.tiles[0].label = "x".repeat(201))),
            ("w and h must be 1 or more", Box::new(|l| l.tiles[0].w = 0)),
            ("w and h must be 1 or more", Box::new(|l| l.tiles[0].h = 0)),
            ("x + w goes past the 12 columns", Box::new(|l| l.tiles[0].x = 10)),
            ("x + w goes past the 12 columns", Box::new(|l| l.tiles[0].x = u32::MAX)),
            ("y and h must be at most", Box::new(|l| l.tiles[0].y = 10_001)),
            ("a tile has 1 to 50 fields", Box::new(|l| l.tiles[0].fields.clear())),
            (
                "app, source and field must be ids",
                Box::new(|l| l.tiles[0].fields[0].app = "My App".into()),
            ),
            (
                "app, source and field must be ids",
                Box::new(|l| l.tiles[0].fields[0].field = String::new()),
            ),
            (
                "a param needs an id as name",
                Box::new(|l| {
                    l.tiles[0].fields[0].params.insert("Bad Name".into(), "x".into());
                }),
            ),
            (
                "there are 501 tiles",
                Box::new(|l| l.tiles = (0..501).map(|n| tile(&format!("t{n}"))).collect()),
            ),
        ];
        for (expected, change) in cases {
            let mut layout = layout(0, &["t1"]);
            change(&mut layout);
            let problems = layout.problems();
            assert!(
                problems.iter().any(|p| p.contains(expected)),
                "expected {expected:?}, got {problems:?}"
            );
        }
    }

    #[test]
    fn a_property_the_layout_does_not_have_is_refused() {
        let with_extra = r#"{"rev": 0, "tiles": [], "theme": "dark"}"#;
        assert!(serde_json::from_str::<Layout>(with_extra).is_err());
        let tile_with_extra =
            r#"{"tiles": [{"id":"t1","view":"number","x":0,"y":0,"w":1,"h":1,"fields":[],"colour":"red"}]}"#;
        assert!(serde_json::from_str::<Layout>(tile_with_extra).is_err());
        let unknown_view = r#"{"tiles": [{"id":"t1","view":"gauge","x":0,"y":0,"w":1,"h":1,"fields":[]}]}"#;
        assert!(serde_json::from_str::<Layout>(unknown_view).is_err());
    }

    #[test]
    fn with_no_file_the_store_starts_from_the_starter() {
        let dir = TempDir::new("starter");
        let store = LayoutStore::open(Some(dir.file("layout.json")), layout(0, &["t1"])).unwrap();
        assert_eq!(store.get(), layout(0, &["t1"]));
        assert!(
            !dir.file("layout.json").exists(),
            "nothing is written before the first save"
        );
    }

    #[test]
    fn a_save_bumps_the_rev_and_survives_a_restart() {
        let dir = TempDir::new("save");
        let path = dir.file("data").join("layout.json"); // the folder does not exist yet
        let store = LayoutStore::open(Some(path.clone()), Layout::default()).unwrap();

        assert_eq!(store.save(layout(0, &["t1"])), Ok(layout(1, &["t1"])));
        assert_eq!(store.save(layout(1, &["t1", "t2"])), Ok(layout(2, &["t1", "t2"])));
        assert_eq!(store.rev(), 2);

        let reopened = LayoutStore::open(Some(path.clone()), Layout::default()).unwrap();
        assert_eq!(reopened.get(), layout(2, &["t1", "t2"]));
        assert!(
            !dir.file("data").join("layout.json.tmp").exists(),
            "the temporary file is gone"
        );
    }

    #[test]
    fn a_save_from_an_old_rev_is_refused() {
        let store = LayoutStore::open(None, Layout::default()).unwrap();
        store.save(layout(0, &["t1"])).unwrap();
        assert_eq!(store.save(layout(0, &["t2"])), Err(SaveError::Conflict { current: 1 }));
        assert_eq!(store.save(layout(7, &["t2"])), Err(SaveError::Conflict { current: 1 }));
        assert_eq!(store.get(), layout(1, &["t1"]), "the refused save changed nothing");
    }

    #[test]
    fn an_invalid_layout_is_not_saved() {
        let store = LayoutStore::open(None, Layout::default()).unwrap();
        let mut bad = layout(0, &["t1"]);
        bad.tiles[0].w = 0;
        assert!(matches!(store.save(bad), Err(SaveError::Invalid(problems)) if problems.len() == 1));
        assert_eq!(store.get(), Layout::default());
    }

    #[test]
    fn a_broken_file_is_an_error_and_is_left_alone() {
        let dir = TempDir::new("broken");
        let path = dir.file("layout.json");

        std::fs::write(&path, "{ not json").unwrap();
        let error = LayoutStore::open(Some(path.clone()), Layout::default()).err().unwrap();
        assert!(error.contains("is not a layout file"), "{error}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");

        std::fs::write(
            &path,
            r#"{"rev": 1, "tiles": [{"id":"t1","view":"number","x":11,"y":0,"w":5,"h":1,"fields":[]}]}"#,
        )
        .unwrap();
        let error = LayoutStore::open(Some(path), Layout::default()).err().unwrap();
        assert!(error.contains("is not a valid layout"), "{error}");
    }

    #[test]
    fn a_layout_kept_in_memory_writes_no_file() {
        let store = LayoutStore::open(None, layout(0, &["t1"])).unwrap();
        assert_eq!(store.save(layout(0, &[])), Ok(layout(1, &[])));
        assert_eq!(store.get(), layout(1, &[]));
    }
}
