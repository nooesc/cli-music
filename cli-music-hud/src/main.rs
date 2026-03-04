mod audio;
mod event_tap;
mod hud;
mod login_item;

use event_tap::VolumeKey;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use objc2_foundation::{NSDate, NSRunLoop};
use std::sync::mpsc;
use std::time::Instant;

/// How long the HUD stays visible before fading out (seconds).
const HUD_DISPLAY_DURATION: f64 = 1.5;

/// How often we poll for events (seconds).
const POLL_INTERVAL: f64 = 0.05;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "install" => {
                login_item::install().unwrap_or_else(|e| eprintln!("Error: {e}"));
                return;
            }
            "uninstall" => {
                login_item::uninstall().unwrap_or_else(|e| eprintln!("Error: {e}"));
                return;
            }
            _ => {
                eprintln!("Usage: cli-music-hud [install|uninstall]");
                return;
            }
        }
    }

    // We must be on the main thread for NSApplication / NSWindow.
    let mtm = MainThreadMarker::new()
        .expect("cli-music-hud must be launched on the main thread");

    // ── 1. NSApplication setup ──────────────────────────────────────────
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    // ── 2. HUD window ───────────────────────────────────────────────────
    let window = hud::create_hud_window(mtm);

    // ── 3. Event tap on a background thread ─────────────────────────────
    let (tx, rx) = mpsc::channel::<VolumeKey>();

    std::thread::spawn(move || {
        if let Err(e) = event_tap::run_event_tap(move |key| {
            let _ = tx.send(key);
        }) {
            eprintln!("Event tap error: {e}");
            std::process::exit(1);
        }
    });

    // ── 4. Main run-loop: poll for volume key events ────────────────────
    let run_loop = NSRunLoop::currentRunLoop();

    // Track when we last showed the HUD so we can auto-hide it.
    let mut hud_shown_at: Option<Instant> = None;

    loop {
        // Let AppKit / CoreAnimation process pending work for up to POLL_INTERVAL.
        let until = NSDate::dateWithTimeIntervalSinceNow(POLL_INTERVAL);
        run_loop.runUntilDate(&until);

        // Process all queued volume-key events.
        while let Ok(key) = rx.try_recv() {
            // Re-query the default device each time in case the user switched
            // audio output (headphones, AirPods, etc.).
            let device = match audio::default_output_device() {
                Some(d) => d,
                None => continue,
            };

            match key {
                VolumeKey::Up => {
                    let vol = audio::get_volume(device).unwrap_or(0.0);
                    let new_vol = (vol + audio::VOLUME_STEP).min(1.0);
                    audio::set_volume(device, new_vol);
                    let muted = audio::is_muted(device).unwrap_or(false);
                    hud::show_hud(&window, new_vol, muted);
                }
                VolumeKey::Down => {
                    let vol = audio::get_volume(device).unwrap_or(0.0);
                    let new_vol = (vol - audio::VOLUME_STEP).max(0.0);
                    audio::set_volume(device, new_vol);
                    let muted = audio::is_muted(device).unwrap_or(false);
                    hud::show_hud(&window, new_vol, muted);
                }
                VolumeKey::Mute => {
                    let muted = audio::is_muted(device).unwrap_or(false);
                    audio::set_mute(device, !muted);
                    let vol = audio::get_volume(device).unwrap_or(0.0);
                    hud::show_hud(&window, vol, !muted);
                }
            }
            hud_shown_at = Some(Instant::now());
        }

        // Auto-hide the HUD after the display duration elapses.
        if let Some(shown_at) = hud_shown_at {
            if shown_at.elapsed().as_secs_f64() >= HUD_DISPLAY_DURATION {
                hud::hide_hud(&window);
                hud_shown_at = None;
            }
        }
    }
}
