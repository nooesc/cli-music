# cli-music: Interactive Apple Music TUI

## Overview

An interactive terminal UI for Apple Music on macOS. Think ncspot/spotify-player but for Apple Music. Controls Music.app via AppleScript/JXA -- it's a rich remote control, not a standalone player.

**macOS only.** Requires Music.app running.

## Architecture

```
┌──────────────────────────────────────────────┐
│              Ratatui TUI Layer               │
│  (event loop, rendering, key handling)       │
├──────────────────────────────────────────────┤
│              App State Layer                 │
│  (current track, library cache, player state)│
├──────────────────────────────────────────────┤
│            Music Bridge Layer                │
│  ┌─────────────────┐  ┌──────────────────┐  │
│  │  apple-music     │  │  Custom JXA      │  │
│  │  crate           │  │  scripts         │  │
│  │  (playback,      │  │  (search, seek,  │  │
│  │   track info,    │  │   large library  │  │
│  │   settings)      │  │   queries)       │  │
│  └────────┬─────────┘  └────────┬─────────┘  │
├───────────┴─────────────────────┴────────────┤
│              Music.app (macOS)               │
└──────────────────────────────────────────────┘
```

### Layers

- **TUI Layer:** ratatui + crossterm. Async event loop handling key events and polling Music.app state on a timer.
- **App State:** Owns all cached data. TUI reads from it, bridge writes to it.
- **Music Bridge:** Thin abstraction over `apple-music` crate + custom JXA scripts. Use the crate where it works, fall back to raw `osascript` for gaps.

### Why hybrid?

The `apple-music` crate (v0.11.5) handles playback control, track info (74 fields), volume/shuffle/repeat, and playlist listing well. But it has limitations:

- ~900 track hard limit on library/playlist fetches
- No global library search (only within a single playlist)
- No seeking (can read position but not set it)
- No queue management
- Blocking I/O only (spawns `osascript` per call)
- Artwork URL fetch can panic on network errors

Custom JXA scripts fill these gaps.

## UI Layout

```
┌─────────────────────────────────────────────────────┐
│  cli-music                                 Playing  │
├──────────────────────┬──────────────────────────────┤
│                      │                              │
│   @@@@@@@@@@@@@@     │  Library / Playlists         │
│   @@@@@@@@@@@@@@     │  ─────────────────────       │
│   @@ ASCII Art @@    │  > All Songs                 │
│   @@ of Album  @@    │    Recently Added            │
│   @@@@@@@@@@@@@@     │    Favorites                 │
│   @@@@@@@@@@@@@@     │    Made For You              │
│                      │    Hip-Hop                   │
│   Track Name         │    Chill Vibes               │
│   Artist - Album     │    Workout                   │
│                      │    ...                       │
│                      │                              │
├──────────────────────┴──────────────────────────────┤
│  <<   >   >>        --*---------- 1:23 / 3:45      │
│  shuf off  repeat off                     vol ####  │
└─────────────────────────────────────────────────────┘
```

### Panels

1. **Left panel** -- Now playing: ASCII art album cover, track name, artist, album.
2. **Right panel** -- Browsable list: playlists at top level, tracks within a playlist on Enter. Navigate with j/k or arrow keys.
3. **Bottom bar** -- Transport status, progress bar, shuffle/repeat indicators, volume level.

## Key Bindings

| Key              | Action                        |
|------------------|-------------------------------|
| `space`          | Toggle play/pause             |
| `n`              | Next track                    |
| `p`              | Previous track                |
| `j` / `Down`     | Navigate list down            |
| `k` / `Up`       | Navigate list up              |
| `Enter`          | Play selected track/playlist  |
| `+` / `=`        | Volume up                     |
| `-`              | Volume down                   |
| `/`              | Search within current playlist|
| `q`              | Quit                          |
| `1`              | Focus left panel              |
| `2`              | Focus right panel             |
| `s`              | Toggle shuffle                |
| `r`              | Cycle repeat mode             |
| `Left`           | Seek backward (custom JXA)    |
| `Right`          | Seek forward (custom JXA)     |

## ASCII Album Art

- Fetch artwork via `apple-music` crate's `fetch_artworks_raw_data()` (local Music.app data)
- Fall back to iTunes Store URL if local fails
- Convert using Unicode half-block characters (▀▄█) with ANSI colors -- 2 pixels per character cell
- Target size: ~30x30 character cells
- Use `image` crate for decode/resize, custom renderer for block art
- Works through tmux (just colored text, no image protocols)
- Graceful fallback: placeholder box with track/album name if no artwork

## Polling & Threading

- Music bridge calls run on a **background thread** via `std::sync::mpsc` channels
- Poll Music.app every **500ms** for current track + player state (one `osascript` call)
- Fetch heavier data (playlist tracks, artwork) **on demand** with loading indicator
- Cache aggressively -- don't re-fetch unchanged data

## Tech Stack

| Component        | Crate / Tool                    |
|------------------|---------------------------------|
| TUI framework    | `ratatui` + `crossterm`         |
| Music.app bridge | `apple-music` crate (v0.11.5)  |
| Custom JXA       | `std::process::Command` + `osascript` |
| Image decode     | `image`                         |
| Serialization    | `serde` + `serde_json`          |
| Error handling   | `anyhow` or `color-eyre`        |

## v1 Scope

**In scope:**
- Interactive TUI with two-panel layout
- Now-playing display with ASCII album art
- Playback controls (play/pause/next/prev/seek)
- Volume, shuffle, repeat controls
- Library browsing (playlists, tracks within playlists)
- Search within a playlist
- Vim-style + arrow key navigation

**Out of scope (future):**
- Queue management
- Playlist creation/editing
- Lyrics display
- Last.fm scrobbling
- Discord rich presence
- Global catalog search (requires Apple Developer account)
- Themes / custom color schemes
