#![allow(non_camel_case_types, dead_code)]

pub type pm_handle = *mut std::ffi::c_void;

extern "C" {
    pub fn pm_create(
        mesh_x: i32,
        mesh_y: i32,
        fps: i32,
        texture_size: i32,
        width: i32,
        height: i32,
        preset_url: *const i8,
        datadir: *const i8,
        smooth_duration: i32,
        preset_duration: i32,
        beat_sensitivity: f32,
    ) -> pm_handle;

    pub fn pm_destroy(handle: pm_handle);
    pub fn pm_render_frame(handle: pm_handle);
    pub fn pm_add_pcm_float(handle: pm_handle, samples: *const f32, count: i32);
    pub fn pm_add_pcm_float_stereo(handle: pm_handle, samples: *const f32, count: i32);
    pub fn pm_reset_gl(handle: pm_handle, width: i32, height: i32);
    pub fn pm_get_playlist_size(handle: pm_handle) -> u32;
    pub fn pm_select_preset(handle: pm_handle, index: u32, hard_cut: bool);
    pub fn pm_select_next(handle: pm_handle, hard_cut: bool);
    pub fn pm_select_previous(handle: pm_handle, hard_cut: bool);
    pub fn pm_set_preset_lock(handle: pm_handle, locked: bool);
    pub fn pm_populate_preset_menu(handle: pm_handle);

    pub fn pm_get_preset_name(handle: pm_handle, index: u32) -> *const i8;
    pub fn pm_get_selected_preset_index(handle: pm_handle, out_index: *mut u32) -> bool;
}
