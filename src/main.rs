mod audio;
mod config;
mod controller;
mod ffi;
mod media;
mod visualizer;

use audio::{compute_rms, AudioCapture};
use config::Config;
use controller::{Controller, Visibility};
use sdl2::event::Event;
use sdl2::video::GLProfile;
use visualizer::Visualizer;

const DEFAULT_W: u32 = 240;
const DEFAULT_H: u32 = 240;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load();
    eprintln!("[pip-milkdrop] Config: sens={:?} delay={}s", config.sensitivity, config.hide_delay_secs);

    let sdl = sdl2::init()?;
    let video_subsystem = sdl.video()?;

    let gl_attr = video_subsystem.gl_attr();
    gl_attr.set_context_profile(GLProfile::Core);
    gl_attr.set_context_version(3, 3);
    gl_attr.set_double_buffer(true);

    let mut window = video_subsystem
        .window("pip-milkdrop", DEFAULT_W, DEFAULT_H)
        .opengl()
        .borderless()
        .resizable()
        .hidden()
        .build()?;

    window.set_always_on_top(true);

    let _gl_ctx = window.gl_create_context()?;
    gl::load_with(|s| video_subsystem.gl_get_proc_address(s) as *const _);

    let preset_path = "/opt/homebrew/share/projectM/presets/presets_stock";
    let viz = Visualizer::new(DEFAULT_W, DEFAULT_H, preset_path)?;

    let mut capture = AudioCapture::new()?;
    capture.start()?;

    media::start_polling();
    eprintln!("[pip-milkdrop] Media polling started.");

    let mut ctrl = Controller::new();
    let mut event_pump = sdl.event_pump()?;

    let mut visible = false;
    let mut dragging = false;
    let mut drag_offset = (0i32, 0i32);
    let mut last_status = std::time::Instant::now();
    let mut total_buffers = 0u64;
    let mut need_resize = false;

    'running: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'running,
                Event::KeyDown {
                    keycode: Some(k), ..
                } => match k {
                    sdl2::keyboard::Keycode::Escape | sdl2::keyboard::Keycode::Q => {
                        break 'running;
                    }
                    sdl2::keyboard::Keycode::Right => viz.select_next(),
                    sdl2::keyboard::Keycode::Left => viz.select_previous(),
                    _ => {}
                },
                Event::MouseButtonDown {
                    x,
                    y,
                    mouse_btn: sdl2::mouse::MouseButton::Left,
                    ..
                } => {
                    dragging = true;
                    drag_offset = (x, y);
                }
                Event::MouseButtonUp {
                    mouse_btn: sdl2::mouse::MouseButton::Left,
                    ..
                } => {
                    dragging = false;
                }
                Event::MouseMotion { x, y, .. } => {
                    if dragging {
                        let pos = window.position();
                        let nx = pos.0 + (x - drag_offset.0);
                        let ny = pos.1 + (y - drag_offset.1);
                        window.set_position(
                            sdl2::video::WindowPos::Positioned(nx),
                            sdl2::video::WindowPos::Positioned(ny),
                        );
                    }
                }
                Event::MouseWheel { y, .. } => {
                    let scale = if y > 0 { 1.15f32 } else { 1f32 / 1.15 };
                    let size = window.size();
                    let nw = ((size.0 as f32) * scale).clamp(100.0, 800.0) as u32;
                    let nh = ((size.1 as f32) * scale).clamp(100.0, 800.0) as u32;
                    window.set_size(nw, nh)?;
                    need_resize = true;
                }
                Event::Window {
                    win_event: sdl2::event::WindowEvent::Resized(w, h),
                    ..
                } => {
                    if w > 0 && h > 0 {
                        need_resize = true;
                    }
                }
                _ => {}
            }
        }

        if need_resize {
            let (w, h) = window.drawable_size();
            viz.reset_gl(w as i32, h as i32);
            need_resize = false;
        }

        let mut latest_rms = 0.0f32;
        let mut audio_buffers = 0u32;
        while let Ok(samples) = capture.rx.try_recv() {
            latest_rms = compute_rms(&samples);
            audio_buffers += 1;
            if visible {
                viz.add_pcm_float_stereo(&samples);
            }
        }
        total_buffers += audio_buffers as u64;

        if last_status.elapsed() >= std::time::Duration::from_secs(3) {
            eprintln!(
                "[pip-milkdrop] rms={latest_rms:.4} media={} vis={visible} buf={total_buffers}",
                media::is_media_playing()
            );
            last_status = std::time::Instant::now();
        }

        let media_playing = media::is_media_playing();
        let changed = ctrl.update(latest_rms, media_playing, &config);
        if changed {
            match ctrl.visibility {
                Visibility::Hidden => {
                    window.hide();
                    visible = false;
                }
                Visibility::Visible => {
                    window.show();
                    window.set_always_on_top(true);
                    visible = true;
                }
            }
        }

        if visible {
            viz.render_frame();
            window.gl_swap_window();
        } else {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    capture.stop();
    Ok(())
}
