use std::fs;
use std::path::PathBuf;

/// Which HUD visual style to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudStyle {
    Legacy,
    Modern,
}

/// Return the path to `~/.config/cli-music/hud.toml`.
fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("cli-music").join("hud.toml"))
}

/// Read the HUD style from the config file.
///
/// Defaults to `Modern` if the file doesn't exist or can't be parsed.
pub fn load_style() -> HudStyle {
    let Some(path) = config_path() else {
        return HudStyle::Modern;
    };

    let Ok(contents) = fs::read_to_string(&path) else {
        return HudStyle::Modern;
    };

    let Ok(table) = contents.parse::<toml::Table>() else {
        return HudStyle::Modern;
    };

    match table.get("style").and_then(|v| v.as_str()) {
        Some("legacy") => HudStyle::Legacy,
        _ => HudStyle::Modern,
    }
}
