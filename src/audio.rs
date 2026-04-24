use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender};

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
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .ok_or("No audio output device found")?;

        eprintln!(
            "[pip-milkdrop] Using output device: {:?}",
            device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "unknown".into())
        );

        let supported_config = device
            .supported_output_configs()
            .map_err(|e| format!("No supported output configs: {e}"))?
            .filter(|c| c.sample_format() == cpal::SampleFormat::F32)
            .next()
            .ok_or("No F32 output config found")?
            .with_max_sample_rate();

        let sample_format = supported_config.sample_format();
        let config = supported_config.config();
        eprintln!(
            "[pip-milkdrop] Audio config: {}Hz, {sample_format:?}, {} channels",
            config.sample_rate, config.channels
        );

        let tx = self.tx.clone();
        let stream = device
            .build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !data.is_empty() {
                        let _ = tx.send(data.to_vec());
                    }
                },
                |err| eprintln!("[pip-milkdrop] Audio error: {err}"),
                None,
            )
            .map_err(|e| format!("Failed to build audio stream: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start audio stream: {e}"))?;

        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.stream.take();
    }
}

pub fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}
