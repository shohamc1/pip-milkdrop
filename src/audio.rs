use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use crossbeam_channel::{Receiver, Sender};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

pub static DEVICE_CHANGED: AtomicBool = AtomicBool::new(false);

#[repr(C)]
#[allow(non_snake_case)]
struct AudioObjectPropertyAddress {
    mSelector: u32,
    mScope: u32,
    mElement: u32,
}

const K_AUDIO_OBJECT_SYSTEM_OBJECT: u32 = 1;
const K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: u32 = 0x64646576;
const K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = 0x676C6F62;
const K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;

extern "C" {
    fn AudioObjectAddPropertyListener(
        inObjectID: u32,
        inAddress: *const AudioObjectPropertyAddress,
        inListener: unsafe extern "C" fn(u32, u32, *const AudioObjectPropertyAddress, *mut c_void) -> i32,
        inClientData: *mut c_void,
    ) -> i32;
}

unsafe extern "C" fn default_device_changed(
    _object_id: u32,
    _num_addresses: u32,
    _addresses: *const AudioObjectPropertyAddress,
    _client_data: *mut c_void,
) -> i32 {
    DEVICE_CHANGED.store(true, Ordering::Relaxed);
    0
}

static LISTENER_REGISTERED: AtomicBool = AtomicBool::new(false);

fn register_device_listener() {
    if LISTENER_REGISTERED.swap(true, Ordering::Relaxed) {
        return;
    }
    let address = AudioObjectPropertyAddress {
        mSelector: K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
        mScope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        mElement: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let result = unsafe {
        AudioObjectAddPropertyListener(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &address,
            default_device_changed,
            std::ptr::null_mut(),
        )
    };
    if result != 0 {
        eprintln!("[pip-milkdrop] Warning: Failed to register device change listener ({result})");
    } else {
        eprintln!("[pip-milkdrop] Registered audio device change listener");
    }
}

pub struct AudioCapture {
    tx: Sender<Vec<f32>>,
    pub rx: Receiver<Vec<f32>>,
    stream: Option<cpal::Stream>,
}

impl AudioCapture {
    pub fn new() -> Result<Self, String> {
        let (tx, rx) = crossbeam_channel::bounded(64);
        Ok(Self {
            tx,
            rx,
            stream: None,
        })
    }

    pub fn start(&mut self) -> Result<(), String> {
        register_device_listener();

        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .ok_or("No audio output device found")?;

        let device_name = device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "unknown".into());
        eprintln!("[pip-milkdrop] Using output device: {device_name}");

        let mut supported_configs = device
            .supported_output_configs()
            .map_err(|e| format!("No supported output configs: {e}"))?;

        let config = supported_configs
            .find(|c| c.sample_format() == SampleFormat::F32)
            .or_else(|| supported_configs.find(|c| c.sample_format() == SampleFormat::I16))
            .or_else(|| supported_configs.next())
            .ok_or("No supported output config found")?
            .with_max_sample_rate();

        let sample_format = config.sample_format();
        let stream_config = config.config();
        eprintln!(
            "[pip-milkdrop] Audio config: {}Hz, {sample_format:?}, {} channels",
            stream_config.sample_rate, stream_config.channels
        );

        let tx = self.tx.clone();

        let stream = match sample_format {
            SampleFormat::F32 => device
                .build_input_stream(
                    &stream_config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if !data.is_empty() {
                            let _ = tx.send(data.to_vec());
                        }
                    },
                    |err| eprintln!("[pip-milkdrop] Audio error: {err}"),
                    None,
                )
                .map_err(|e| format!("Failed to build audio stream: {e}"))?,
            SampleFormat::I16 => device
                .build_input_stream(
                    &stream_config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if data.is_empty() {
                            return;
                        }
                        let converted: Vec<f32> = data
                            .iter()
                            .map(|&s| s as f32 / i16::MAX as f32)
                            .collect();
                        let _ = tx.send(converted);
                    },
                    |err| eprintln!("[pip-milkdrop] Audio error: {err}"),
                    None,
                )
                .map_err(|e| format!("Failed to build audio stream: {e}"))?,
            _ => return Err(format!("Unsupported sample format: {sample_format:?}")),
        };

        stream
            .play()
            .map_err(|e| format!("Failed to start audio stream: {e}"))?;

        self.stream = Some(stream);
        eprintln!("[pip-milkdrop] Audio capture started on: {device_name}");
        Ok(())
    }

    pub fn stop(&mut self) {
        self.stream.take();
    }

    pub fn restart(&mut self) -> Result<(), String> {
        self.stop();
        self.start()
    }
}

pub fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}
