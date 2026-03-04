use std::fs;
use std::path::PathBuf;

const LABEL: &str = "com.cli-music.hud";

fn plist_path() -> PathBuf {
    let home = dirs::home_dir().expect("No home directory");
    home.join("Library/LaunchAgents").join(format!("{LABEL}.plist"))
}

fn app_path() -> String {
    "/Applications/VolumeHUD.app/Contents/MacOS/cli-music-hud".to_string()
}

pub fn install() -> Result<(), String> {
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>"#,
        app_path()
    );

    let path = plist_path();
    fs::write(&path, plist).map_err(|e| format!("Failed to write plist: {e}"))?;

    println!("Installed login item at {}", path.display());
    println!("VolumeHUD will start automatically on next login.");
    Ok(())
}

pub fn uninstall() -> Result<(), String> {
    let path = plist_path();
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Failed to remove plist: {e}"))?;
        println!("Removed login item.");
    } else {
        println!("No login item found.");
    }
    Ok(())
}
