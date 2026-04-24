use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_sensitivity")]
    pub sensitivity: Sensitivity,
    #[serde(default = "default_hide_delay")]
    pub hide_delay_secs: u64,
    #[serde(default)]
    pub start_at_login: bool,
    #[serde(default)]
    pub locked_preset_index: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sensitivity {
    Low,
    Medium,
    High,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sensitivity: default_sensitivity(),
            hide_delay_secs: default_hide_delay(),
            start_at_login: false,
            locked_preset_index: None,
        }
    }
}

fn default_sensitivity() -> Sensitivity {
    Sensitivity::Medium
}

fn default_hide_delay() -> u64 {
    4
}

impl Config {
    pub fn rms_threshold(&self) -> f32 {
        match self.sensitivity {
            Sensitivity::Low => 0.02,
            Sensitivity::Medium => 0.01,
            Sensitivity::High => 0.003,
        }
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        match fs::read_to_string(&path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self) {
        if let Some(dir) = Self::config_dir() {
            let _ = fs::create_dir_all(&dir);
            let _ = fs::write(Self::config_path(), serde_json::to_string_pretty(self).unwrap_or_default());
        }
    }

    fn config_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("pip-milkdrop"))
    }

    fn config_path() -> PathBuf {
        Self::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("config.json")
    }
}

pub fn update_launch_agent(enabled: bool) {
    let plist_path = dirs::home_dir()
        .map(|p| p.join("Library/LaunchAgents/com.pip-milkdrop.plist"))
        .unwrap();

    if enabled {
        let exe = std::env::current_exe().unwrap_or_default();
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.pip-milkdrop</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#,
            exe.display()
        );
        let _ = fs::write(&plist_path, plist);
    } else {
        let _ = fs::remove_file(&plist_path);
    }
}
