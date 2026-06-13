use std::ffi::{CStr, CString};
use std::path::Path;

use crate::ffi;

/// Safe wrapper over a projectM 4.x instance. The main visualizer also owns a connected
/// playlist that drives index-based preset navigation; lightweight preview-pool instances
/// (`new_thumbnail`) have no playlist and load presets directly by file path.
pub struct Visualizer {
    handle: ffi::projectm_handle,
    playlist: ffi::projectm_playlist_handle,
}

impl Visualizer {
    pub fn new(width: u32, height: u32, preset_path: &str) -> Result<Self, String> {
        let handle = unsafe { ffi::projectm_create() };
        if handle.is_null() {
            return Err("Failed to create projectM instance".into());
        }
        unsafe {
            ffi::projectm_set_window_size(handle, width as usize, height as usize);
            ffi::projectm_set_mesh_size(handle, 48, 32);
            ffi::projectm_set_fps(handle, 60);
            ffi::projectm_set_aspect_correction(handle, true);
            ffi::projectm_set_beat_sensitivity(handle, 1.0);
            // We drive preset selection ourselves: no auto-advance, no soft-cut blend.
            ffi::projectm_set_preset_duration(handle, 999_999.0);
            ffi::projectm_set_soft_cut_duration(handle, 0.0);
            ffi::projectm_set_hard_cut_enabled(handle, false);
        }
        let playlist = unsafe { ffi::projectm_playlist_create(handle) };
        if playlist.is_null() {
            unsafe { ffi::projectm_destroy(handle) };
            return Err("Failed to create projectM playlist".into());
        }

        let viz = Self { handle, playlist };
        viz.add_path(preset_path);
        viz.sort_playlist();
        Ok(viz)
    }

    /// A lighter-weight instance for the gallery preview pool: smaller mesh/texture, no
    /// playlist. Presets are loaded directly by path via [`load_preset_file`].
    pub fn new_thumbnail(width: u32, height: u32) -> Result<Self, String> {
        let handle = unsafe { ffi::projectm_create() };
        if handle.is_null() {
            return Err("Failed to create projectM preview instance".into());
        }
        unsafe {
            ffi::projectm_set_window_size(handle, width as usize, height as usize);
            ffi::projectm_set_mesh_size(handle, 32, 24);
            ffi::projectm_set_fps(handle, 30);
            ffi::projectm_set_aspect_correction(handle, true);
            ffi::projectm_set_beat_sensitivity(handle, 1.0);
            ffi::projectm_set_soft_cut_duration(handle, 0.0);
            ffi::projectm_set_hard_cut_enabled(handle, false);
        }
        Ok(Self {
            handle,
            playlist: std::ptr::null_mut(),
        })
    }

    fn add_path(&self, path: &str) {
        if self.playlist.is_null() {
            return;
        }
        if let Ok(c) = CString::new(path) {
            unsafe { ffi::projectm_playlist_add_path(self.playlist, c.as_ptr(), true, false) };
        }
    }

    fn sort_playlist(&self) {
        if self.playlist.is_null() {
            return;
        }
        unsafe {
            let n = ffi::projectm_playlist_size(self.playlist);
            ffi::projectm_playlist_sort(
                self.playlist,
                0,
                n,
                ffi::SORT_PREDICATE_FILENAME_ONLY,
                ffi::SORT_ORDER_ASCENDING,
            );
        }
    }

    pub fn load_preset_file(&self, path: &str, smooth: bool) {
        if let Ok(c) = CString::new(path) {
            unsafe { ffi::projectm_load_preset_file(self.handle, c.as_ptr(), smooth) };
        }
    }

    pub fn render_frame(&self) {
        unsafe { ffi::projectm_opengl_render_frame(self.handle) };
    }

    #[allow(dead_code)]
    pub fn render_frame_fbo(&self, fbo: u32) {
        unsafe { ffi::projectm_opengl_render_frame_fbo(self.handle, fbo) };
    }

    #[allow(dead_code)]
    pub fn add_pcm_float(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        unsafe {
            ffi::projectm_pcm_add_float(
                self.handle,
                samples.as_ptr(),
                samples.len() as u32,
                ffi::PROJECTM_MONO,
            );
        }
    }

    pub fn add_pcm_float_stereo(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        // `count` is the number of samples per channel; `samples` is interleaved LRLR.
        let frames = (samples.len() / 2) as u32;
        unsafe {
            ffi::projectm_pcm_add_float(
                self.handle,
                samples.as_ptr(),
                frames,
                ffi::PROJECTM_STEREO,
            );
        }
    }

    pub fn reset_gl(&self, width: i32, height: i32) {
        unsafe {
            ffi::projectm_set_window_size(
                self.handle,
                width.max(0) as usize,
                height.max(0) as usize,
            )
        };
    }

    pub fn select_preset(&self, index: u32) {
        if self.playlist.is_null() {
            return;
        }
        unsafe {
            ffi::projectm_playlist_set_position(self.playlist, index, true);
            ffi::projectm_set_preset_locked(self.handle, true);
        }
    }

    pub fn select_next(&self) {
        if !self.playlist.is_null() {
            unsafe { ffi::projectm_playlist_play_next(self.playlist, true) };
        }
    }

    pub fn select_random_from(&self, indices: &[u32]) {
        if indices.is_empty() {
            return;
        }
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let i = (ns as usize) % indices.len();
        self.select_preset(indices[i]);
    }

    pub fn select_previous(&self) {
        if !self.playlist.is_null() {
            unsafe { ffi::projectm_playlist_play_previous(self.playlist, true) };
        }
    }

    pub fn playlist_size(&self) -> u32 {
        if self.playlist.is_null() {
            return 0;
        }
        unsafe { ffi::projectm_playlist_size(self.playlist) }
    }

    /// Absolute file path of the preset at `index` in the playlist.
    pub fn preset_path(&self, index: u32) -> Option<String> {
        if self.playlist.is_null() {
            return None;
        }
        unsafe {
            let ptr = ffi::projectm_playlist_item(self.playlist, index);
            if ptr.is_null() {
                return None;
            }
            let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
            ffi::projectm_playlist_free_string(ptr);
            Some(s)
        }
    }

    /// Display name (file name) of the preset at `index`, matching the 3.x behaviour the
    /// rest of the app (favourites, gallery labels) keys on.
    pub fn preset_name(&self, index: u32) -> String {
        self.preset_path(index)
            .and_then(|p| {
                Path::new(&p)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default()
    }

    pub fn load_user_presets(&self, dir: &str) {
        if self.playlist.is_null() || !Path::new(dir).is_dir() {
            return;
        }
        // Append after the stock presets and sort only the newly added range, so the
        // stock/user split (the gallery's section boundary) stays at `stock_count`.
        let before = self.playlist_size();
        self.add_path(dir);
        let after = self.playlist_size();
        if after > before {
            unsafe {
                ffi::projectm_playlist_sort(
                    self.playlist,
                    before,
                    after - before,
                    ffi::SORT_PREDICATE_FILENAME_ONLY,
                    ffi::SORT_ORDER_ASCENDING,
                );
            }
        }
    }

    pub fn selected_preset_index(&self) -> u32 {
        if self.playlist.is_null() {
            return 0;
        }
        unsafe { ffi::projectm_playlist_get_position(self.playlist) }
    }
}

impl Drop for Visualizer {
    fn drop(&mut self) {
        unsafe {
            if !self.playlist.is_null() {
                ffi::projectm_playlist_destroy(self.playlist);
            }
            if !self.handle.is_null() {
                ffi::projectm_destroy(self.handle);
            }
        }
    }
}

unsafe impl Send for Visualizer {}
unsafe impl Sync for Visualizer {}
