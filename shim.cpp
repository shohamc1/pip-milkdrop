#include "libprojectM/projectM.hpp"
#include "libprojectM/PCM.hpp"
#include <cstdint>

extern "C" {

typedef void* pm_handle_t;

pm_handle_t pm_create(int mesh_x, int mesh_y, int fps, int texture_size,
                      int width, int height, const char* preset_url,
                      const char* datadir, int smooth_duration,
                      int preset_duration, float beat_sensitivity) {
    projectM::Settings settings;
    settings.meshX = mesh_x;
    settings.meshY = mesh_y;
    settings.fps = fps;
    settings.textureSize = texture_size;
    settings.windowWidth = width;
    settings.windowHeight = height;
    if (preset_url) settings.presetURL = preset_url;
    if (datadir) settings.datadir = datadir;
    settings.smoothPresetDuration = smooth_duration;
    settings.presetDuration = preset_duration;
    settings.beatSensitivity = beat_sensitivity;
    settings.shuffleEnabled = false;
    try {
        return static_cast<void*>(new projectM(settings, projectM::FLAG_NONE));
    } catch (...) {
        return nullptr;
    }
}

void pm_destroy(pm_handle_t handle) {
    delete static_cast<projectM*>(handle);
}

void pm_render_frame(pm_handle_t handle) {
    static_cast<projectM*>(handle)->renderFrame();
}

void pm_add_pcm_float(pm_handle_t handle, const float* samples, int count) {
    static_cast<projectM*>(handle)->pcm()->addPCMfloat(samples, count);
}

void pm_add_pcm_float_stereo(pm_handle_t handle, const float* samples, int count) {
    static_cast<projectM*>(handle)->pcm()->addPCMfloat_2ch(samples, count);
}

void pm_reset_gl(pm_handle_t handle, int width, int height) {
    static_cast<projectM*>(handle)->projectM_resetGL(width, height);
}

uint32_t pm_get_playlist_size(pm_handle_t handle) {
    return static_cast<uint32_t>(static_cast<projectM*>(handle)->getPlaylistSize());
}

void pm_select_preset(pm_handle_t handle, uint32_t index, bool hard_cut) {
    static_cast<projectM*>(handle)->selectPreset(index, hard_cut);
}

void pm_select_next(pm_handle_t handle, bool hard_cut) {
    static_cast<projectM*>(handle)->selectNext(hard_cut);
}

void pm_select_previous(pm_handle_t handle, bool hard_cut) {
    static_cast<projectM*>(handle)->selectPrevious(hard_cut);
}

void pm_set_preset_lock(pm_handle_t handle, bool locked) {
    static_cast<projectM*>(handle)->setPresetLock(locked);
}

const char* pm_get_preset_name(pm_handle_t handle, unsigned int index) {
    static thread_local std::string stored;
    stored = static_cast<projectM*>(handle)->getPresetName(index);
    return stored.c_str();
}

bool pm_get_selected_preset_index(pm_handle_t handle, unsigned int* out_index) {
    return static_cast<projectM*>(handle)->selectedPresetIndex(*out_index);
}

void pm_populate_preset_menu(pm_handle_t handle) {
    static_cast<projectM*>(handle)->populatePresetMenu();
}

}
