//! What the user chose, saved as JSON in the app's data folder.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::game::cheats::CheatId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scenery {
    /// The lily field under the moon.
    #[default]
    Night,
    /// The overgrown ruins in the fog.
    Ruins,
    /// The user's own picture.
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub scenery: Scenery,
    pub custom_background: Option<PathBuf>,
    pub animated: bool,
    pub hotkeys: bool,
    pub auto_collect: bool,
    pub range_on: bool,
    pub respawn_on: bool,
    pub item_cap_on: bool,
    pub range: f32,
    pub item_cap: u32,
    pub respawn_auto: bool,
    pub respawn_delay: f32,
    pub respawn_at_death: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scenery: Scenery::Night,
            custom_background: None,
            animated: true,
            hotkeys: true,
            auto_collect: false,
            range_on: false,
            respawn_on: false,
            item_cap_on: false,
            range: 100.0,
            item_cap: 9999,
            respawn_auto: true,
            respawn_delay: 3.0,
            respawn_at_death: true,
        }
    }
}

impl Settings {
    pub fn enabled(&self, id: CheatId) -> bool {
        match id {
            CheatId::AutoCollect => self.auto_collect,
            CheatId::Range => self.range_on,
            CheatId::Respawn => self.respawn_on,
            CheatId::ItemCap => self.item_cap_on,
        }
    }

    pub fn set_enabled(&mut self, id: CheatId, on: bool) {
        match id {
            CheatId::AutoCollect => self.auto_collect = on,
            CheatId::Range => self.range_on = on,
            CheatId::Respawn => self.respawn_on = on,
            CheatId::ItemCap => self.item_cap_on = on,
        }
    }

    pub fn load() -> Self {
        let Some(path) = file() else {
            return Self::default();
        };
        // (Notepad and PowerShell may save with a BOM, which serde_json rejects.)
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = file() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, text);
        }
    }
}

fn file() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "yorha-trainer")
        .map(|d| d.config_dir().join("settings.json"))
}
