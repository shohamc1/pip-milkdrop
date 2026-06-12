use crate::ffi;

pub struct Visualizer {
    handle: ffi::pm_handle,
}

impl Visualizer {
    pub fn new(width: u32, height: u32, preset_path: &str) -> Result<Self, String> {
        let c_preset = std::ffi::CString::new(preset_path).map_err(|e| e.to_string())?;
        let c_datadir =
            std::ffi::CString::new(env!("PROJECTM_DATADIR")).map_err(|e| e.to_string())?;

        let handle = unsafe {
            ffi::pm_create(
                48,
                36,
                60,
                1024,
                width as i32,
                height as i32,
                c_preset.as_ptr(),
                c_datadir.as_ptr(),
                10,
                15,
                1.0,
            )
        };

        if handle.is_null() {
            return Err("Failed to create projectM instance".into());
        }

        Ok(Self { handle })
    }

    pub fn render_frame(&self) {
        unsafe {
            ffi::pm_render_frame(self.handle);
        }
    }

    #[allow(dead_code)]
    pub fn add_pcm_float(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        unsafe {
            ffi::pm_add_pcm_float(self.handle, samples.as_ptr(), samples.len() as i32);
        }
    }

    pub fn add_pcm_float_stereo(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        unsafe {
            ffi::pm_add_pcm_float_stereo(self.handle, samples.as_ptr(), samples.len() as i32);
        }
    }

    pub fn reset_gl(&self, width: i32, height: i32) {
        unsafe {
            ffi::pm_reset_gl(self.handle, width, height);
        }
    }

    pub fn select_preset(&self, index: u32) {
        unsafe {
            ffi::pm_select_preset(self.handle, index, true);
            ffi::pm_set_preset_lock(self.handle, true);
        }
    }

    pub fn select_next(&self) {
        unsafe {
            ffi::pm_select_next(self.handle, true);
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
        unsafe {
            ffi::pm_select_previous(self.handle, true);
        }
    }

    pub fn playlist_size(&self) -> u32 {
        unsafe { ffi::pm_get_playlist_size(self.handle) }
    }

    pub fn preset_name(&self, index: u32) -> String {
        unsafe {
            let ptr = ffi::pm_get_preset_name(self.handle, index);
            if ptr.is_null() {
                return String::new();
            }
            std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }

    pub fn load_user_presets(&self, dir: &str) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "milk" && ext != "milk2" && ext != "prjm" {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let url = path.to_string_lossy().into_owned();
            let c_url = match std::ffi::CString::new(url.as_str()) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let c_name = match std::ffi::CString::new(name.as_str()) {
                Ok(c) => c,
                Err(_) => continue,
            };
            unsafe {
                ffi::pm_add_preset_url(self.handle, c_url.as_ptr(), c_name.as_ptr());
            }
        }
    }

    pub fn selected_preset_index(&self) -> u32 {
        unsafe {
            let mut idx: u32 = 0;
            ffi::pm_get_selected_preset_index(self.handle, &mut idx);
            idx
        }
    }
}

impl Drop for Visualizer {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                ffi::pm_destroy(self.handle);
            }
        }
    }
}

unsafe impl Send for Visualizer {}
unsafe impl Sync for Visualizer {}
