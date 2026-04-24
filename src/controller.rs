use std::time::Instant;

use crate::config::Config;

const SHOW_FRAMES_NEEDED: u32 = 5;

pub enum Visibility {
    Hidden,
    Visible,
}

pub struct Controller {
    pub visibility: Visibility,
    consecutive_loud: u32,
    silence_since: Option<Instant>,
}

impl Controller {
    pub fn new() -> Self {
        Self {
            visibility: Visibility::Hidden,
            consecutive_loud: 0,
            silence_since: None,
        }
    }

    pub fn update(&mut self, rms: f32, media_playing: bool, config: &Config) -> bool {
        let now = Instant::now();
        let mut changed = false;

        let threshold = config.rms_threshold();
        let media_or_audio = rms >= threshold || media_playing;

        if media_or_audio {
            self.consecutive_loud = self.consecutive_loud.saturating_add(1);
            self.silence_since = None;
        } else {
            self.consecutive_loud = 0;
            if self.silence_since.is_none() {
                self.silence_since = Some(now);
            }
        }

        match self.visibility {
            Visibility::Hidden => {
                if self.consecutive_loud >= SHOW_FRAMES_NEEDED {
                    self.visibility = Visibility::Visible;
                    self.silence_since = None;
                    changed = true;
                }
            }
            Visibility::Visible => {
                if let Some(since) = self.silence_since {
                    let should_hide = if config.hide_delay_secs == 0 {
                        false
                    } else {
                        now.duration_since(since).as_secs_f64() >= config.hide_delay_secs as f64
                    };
                    if should_hide {
                        self.visibility = Visibility::Hidden;
                        changed = true;
                    }
                }
            }
        }

        changed
    }
}
