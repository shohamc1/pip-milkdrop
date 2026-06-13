#![allow(non_camel_case_types, dead_code)]

//! Direct bindings to the projectM 4.x C API (core + playlist). projectM 4.x ships a
//! stable C API, so there is no C++ shim — these `extern` declarations bind it directly.

use std::ffi::{c_char, c_void};

pub type projectm_handle = *mut c_void;
pub type projectm_playlist_handle = *mut c_void;

// projectm_channels
pub const PROJECTM_MONO: u32 = 1;
pub const PROJECTM_STEREO: u32 = 2;

// projectm_playlist_sort_predicate
pub const SORT_PREDICATE_FULL_PATH: u32 = 0;
pub const SORT_PREDICATE_FILENAME_ONLY: u32 = 1;
// projectm_playlist_sort_order
pub const SORT_ORDER_ASCENDING: u32 = 0;
pub const SORT_ORDER_DESCENDING: u32 = 1;

extern "C" {
    // --- core ---
    pub fn projectm_create() -> projectm_handle;
    pub fn projectm_destroy(instance: projectm_handle);
    pub fn projectm_load_preset_file(
        instance: projectm_handle,
        filename: *const c_char,
        smooth_transition: bool,
    );
    pub fn projectm_reset_textures(instance: projectm_handle);

    // --- rendering ---
    pub fn projectm_opengl_render_frame(instance: projectm_handle);
    pub fn projectm_opengl_render_frame_fbo(instance: projectm_handle, framebuffer_object_id: u32);

    // --- audio ---
    pub fn projectm_pcm_add_float(
        instance: projectm_handle,
        samples: *const f32,
        count: u32,
        channels: u32,
    );

    // --- parameters ---
    pub fn projectm_set_window_size(instance: projectm_handle, width: usize, height: usize);
    pub fn projectm_set_mesh_size(instance: projectm_handle, width: usize, height: usize);
    pub fn projectm_set_fps(instance: projectm_handle, fps: i32);
    pub fn projectm_set_preset_duration(instance: projectm_handle, seconds: f64);
    pub fn projectm_set_soft_cut_duration(instance: projectm_handle, seconds: f64);
    pub fn projectm_set_hard_cut_enabled(instance: projectm_handle, enabled: bool);
    pub fn projectm_set_beat_sensitivity(instance: projectm_handle, sensitivity: f32);
    pub fn projectm_set_aspect_correction(instance: projectm_handle, enabled: bool);
    pub fn projectm_set_preset_locked(instance: projectm_handle, lock: bool);

    // --- playlist ---
    pub fn projectm_playlist_create(instance: projectm_handle) -> projectm_playlist_handle;
    pub fn projectm_playlist_destroy(playlist: projectm_playlist_handle);
    pub fn projectm_playlist_add_path(
        playlist: projectm_playlist_handle,
        path: *const c_char,
        recurse_subdirs: bool,
        allow_duplicates: bool,
    ) -> u32;
    pub fn projectm_playlist_size(playlist: projectm_playlist_handle) -> u32;
    pub fn projectm_playlist_item(playlist: projectm_playlist_handle, index: u32) -> *mut c_char;
    pub fn projectm_playlist_set_position(
        playlist: projectm_playlist_handle,
        new_position: u32,
        hard_cut: bool,
    ) -> u32;
    pub fn projectm_playlist_get_position(playlist: projectm_playlist_handle) -> u32;
    pub fn projectm_playlist_play_next(playlist: projectm_playlist_handle, hard_cut: bool) -> u32;
    pub fn projectm_playlist_play_previous(
        playlist: projectm_playlist_handle,
        hard_cut: bool,
    ) -> u32;
    pub fn projectm_playlist_set_shuffle(playlist: projectm_playlist_handle, shuffle: bool);
    pub fn projectm_playlist_sort(
        playlist: projectm_playlist_handle,
        start_index: u32,
        count: u32,
        predicate: u32,
        order: u32,
    );
    pub fn projectm_playlist_free_string(string: *mut c_char);
}
