# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`pip-milkdrop` is a macOS-only picture-in-picture MilkDrop visualizer. It shows a small always-on-top floating window that renders [projectM](https://github.com/projectM-visualizer/projectm) presets reacting to the system's audio output, plus a menu-bar item and a fullscreen preset gallery browser. The window auto-shows when audio/media is playing and auto-hides after a configurable delay.

## Build & run

```bash
brew install projectm pkg-config   # required system deps (libprojectM resolved via pkg-config)
cargo build                        # debug
cargo build --release              # release
cargo run                          # run

# Debug window: forces the visualizer always-visible and feeds synthesized audio
cargo run -- --debug               # or: PIP_MILKDROP_DEBUG=1 cargo run
```

There is **no test suite** (`cargo test` runs nothing). Use `cargo clippy` / `cargo fmt` for lint/format. `cargo check` is the fast feedback loop.

`build.rs` probes `libprojectM` with `pkg-config`, compiles `shim.cpp`, and exposes the projectM preset data dir to the program as the `PROJECTM_DATADIR` compile-time env var (read via `env!("PROJECTM_DATADIR")`). Stock presets load from `$PROJECTM_DATADIR/presets/presets_stock`.

## Architecture

This is a **single-threaded, manually-driven AppKit app**. There is no `NSApplication.run()`; `main.rs` owns a custom run loop that each iteration: drains pending `NSEvent`s, applies menu/gallery actions, pumps audio into projectM, advances the controller's show/hide state machine, and renders a frame. AppKit/objc2 bindings are used directly (no SDL, no winit).

### The projectM FFI seam
- `shim.cpp` — thin `extern "C"` wrapper over the C++ `projectM` class (this targets the **projectM 3.x C++ `Settings`/`projectM` API**, not the newer C API).
- `src/ffi.rs` — Rust `extern` declarations matching the shim.
- `src/visualizer.rs` — safe `Visualizer` wrapper owning the opaque handle (PCM feeding, preset selection/locking, playlist queries, user-preset loading). All OpenGL calls happen on the main thread against the single shared `NSOpenGLContext`; `Visualizer` is marked `Send`/`Sync` but must only be driven from the main loop.

### Cross-component communication = global atomics
Objective-C callbacks (menu clicks, gallery card clicks, hover, audio device changes, media playback state) cannot easily call back into Rust state, so they write to **`static` atomics** that the main loop polls and swaps each iteration:
- `menubar::PENDING_ACTION` — tag of the last clicked menu item (`TAG_*` constants).
- `gallery::GALLERY_ACTION` — encoded gallery action; ranges are partitioned by base constants (`GA_SELECT_BASE`, `GA_FAV_BASE`, `GA_SECTION_BASE`, …) so one i32 carries both an action kind and an index.
- `gallery::GALLERY_HOVER` — index of the hovered card, or `-1`.
- `audio::DEVICE_CHANGED` — set by a CoreAudio property listener when the default output device changes; triggers `AudioCapture::restart()`.
- `media::IS_PLAYING` — updated by the media poller thread.

When adding a new menu/gallery interaction, follow this pattern: define a tag/base constant, store it from the objc2 callback, and handle it in the corresponding match arm in `main.rs` (or `menubar::handle_pending_action`).

### Show/hide logic
`src/controller.rs` is a small state machine: needs `SHOW_FRAMES_NEEDED` consecutive "loud or media-playing" frames to show, and `hide_delay_secs` of silence to hide. "Loud" is `rms >= config.rms_threshold()`, where the threshold is derived from the `Sensitivity` enum. `main.rs` layers extra rules on top (e.g. if the user manually closes the window, `user_dismissed_window` suppresses re-showing until activity stops).

### Audio
`src/audio.rs` captures via `cpal` from the **default output device** (system audio). It registers a CoreAudio default-output-device-changed listener directly via `extern "C"` (`AudioObjectAddPropertyListener`). Samples flow over a bounded `crossbeam-channel` to the main loop, which computes RMS and forwards stereo PCM to projectM.

### Media state
`src/media.rs` polls the **private `MediaRemote` framework** (`MRMediaRemoteGetNowPlayingApplicationIsPlaying`) on a background thread to know whether anything is "now playing", independent of measured loudness.

### Gallery
`src/gallery.rs` (the largest module) is the fullscreen preset browser: an `NSWindow` of card views with search, All/Favorites tabs, collapsible stock/user sections, and live thumbnail previews. Thumbnails are produced by selecting a preset, rendering a few frames with synthesized audio, and reading back the GL framebuffer (`capture_gl_image`); results are cached to disk. Hover previews render on demand. Because it shares the one GL context with the main visualizer, the gallery saves/restores `selected_preset_index` around every off-screen render.

### Config & persistence
`src/config.rs` — `Config` (sensitivity, hide delay, favorites set, shuffle mode, start-at-login, locked preset) serialized to `~/Library/Application Support/pip-milkdrop/config.json`. User-supplied presets (`.milk`/`.milk2`/`.prjm`) load from `~/Library/Application Support/pip-milkdrop/presets/`. `update_launch_agent` writes/removes a `LaunchAgents` plist for start-at-login.

## Conventions

- objc2 message sends use `msg_send![...]` and require correct `MainThreadMarker` threading; almost everything runs on the main thread by design.
- Cross-thread/callback state is shared via `static` atomics, never locks — keep that pattern.
- Logging is `eprintln!("[pip-milkdrop] ...")` to stderr; there's a periodic (~3s) status line in the main loop.
