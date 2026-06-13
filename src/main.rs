#![allow(deprecated)]

mod audio;
mod config;
mod controller;
mod ffi;
mod gallery;
mod media;
mod menubar;
mod visualizer;

use audio::{compute_rms, AudioCapture};
use config::Config;
use controller::{Controller, Visibility};
use visualizer::Visualizer;

use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::AnyObject;
use objc2::{class, define_class, msg_send, AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSEventMask,
    NSFloatingWindowLevel, NSOpenGLContext, NSOpenGLContextParameter, NSOpenGLPFAAlphaSize,
    NSOpenGLPFAColorSize, NSOpenGLPFADepthSize, NSOpenGLPFADoubleBuffer, NSOpenGLPFAOpenGLProfile,
    NSOpenGLPixelFormat, NSOpenGLProfileVersion4_1Core, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSPoint, NSRect};

use crate::gallery::Gallery;
use crate::menubar::MenuBar;

const DEFAULT_W: f64 = 240.0;
const DEFAULT_H: f64 = 240.0;

extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const i8) -> *mut c_void;
}

define_class!(
    #[unsafe(super(NSView))]
    struct VizView;

    impl VizView {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        // The visualizer content fills the whole floating window. Without this, macOS
        // treats the custom OpenGL view as an interactive child and the user's drag does
        // not consistently move the borderless-looking floating window.
        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&objc2_app_kit::NSEvent>) -> bool {
            true
        }
    }
);

#[derive(Debug, Clone, Copy)]
struct DebugMode {
    enabled: bool,
    always_show: bool,
    simulated_audio: bool,
}

impl DebugMode {
    fn from_env_and_args() -> Self {
        let enabled = std::env::args().any(|arg| arg == "--debug" || arg == "--debug-window")
            || std::env::var("PIP_MILKDROP_DEBUG").is_ok_and(|v| v != "0");
        Self {
            enabled,
            always_show: enabled,
            simulated_audio: enabled,
        }
    }
}

fn generate_debug_audio(time: &mut f64) -> Vec<f32> {
    let frames = 1024usize;
    let sample_rate = 44_100.0f64;
    let mut out = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let t = *time + n as f64 / sample_rate;
        let beat = (t * 2.0 * std::f64::consts::PI * 1.8).sin().max(0.0) as f32;
        let amp = 0.08 + 0.25 * beat;
        let left = ((t * 2.0 * std::f64::consts::PI * 110.0).sin() as f32) * amp;
        let right = ((t * 2.0 * std::f64::consts::PI * 147.0).sin() as f32) * amp;
        out.push(left);
        out.push(right);
    }
    *time += frames as f64 / sample_rate;
    out
}

fn main() {
    let debug = DebugMode::from_env_and_args();
    if debug.enabled {
        eprintln!("[pip-milkdrop] DEBUG MODE: forcing visualizer visible and simulating audio (--debug/PIP_MILKDROP_DEBUG)");
    }
    let config = Config::load();
    eprintln!(
        "[pip-milkdrop] Config: sens={:?} delay={}s",
        config.sensitivity, config.hide_delay_secs
    );

    let mtm = MainThreadMarker::new().expect("Must run on main thread");

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.finishLaunching();

    let attrs: Vec<u32> = vec![
        NSOpenGLPFAOpenGLProfile,
        NSOpenGLProfileVersion4_1Core,
        NSOpenGLPFADoubleBuffer,
        NSOpenGLPFAColorSize,
        24,
        NSOpenGLPFAAlphaSize,
        8,
        NSOpenGLPFADepthSize,
        16,
        0,
    ];

    let pixel_format = unsafe {
        NSOpenGLPixelFormat::initWithAttributes(
            NSOpenGLPixelFormat::alloc(),
            std::ptr::NonNull::new(attrs.as_ptr() as *mut u32).unwrap(),
        )
    }
    .expect("Failed to create NSOpenGLPixelFormat");

    let screen_frame: NSRect = unsafe {
        let screen: Option<Retained<AnyObject>> = msg_send![class!(NSScreen), mainScreen];
        let screen = screen.expect("No main screen");
        let frame: NSRect = msg_send![&*screen, frame];
        frame
    };

    let initial_x = screen_frame.origin.x + screen_frame.size.width - DEFAULT_W - 20.0;
    let initial_y = screen_frame.origin.y + 20.0;
    let window_rect = NSRect::new(
        NSPoint::new(initial_x, initial_y),
        CGSize::new(DEFAULT_W, DEFAULT_H),
    );

    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Resizable
        | NSWindowStyleMask::FullSizeContentView;
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            window_rect,
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };

    window.setLevel(NSFloatingWindowLevel);
    window.setOpaque(true);
    window.setHasShadow(true);
    window.setMinSize(CGSize::new(160.0, 160.0));
    window.setMaxSize(CGSize::new(800.0, 800.0));
    unsafe {
        let _: () = msg_send![&window, setReleasedWhenClosed: false];
        let _: () =
            msg_send![&window, setTitle: &*objc2_foundation::NSString::from_str("pip-milkdrop")];
        let _: () = msg_send![&window, setTitlebarAppearsTransparent: true];
        let _: () = msg_send![&window, setTitleVisibility: 1u64]; // NSWindowTitleHidden
        let _: () = msg_send![&window, setMovableByWindowBackground: true];
        let _: () = msg_send![&window, setShowsResizeIndicator: false];
        for button_id in 0u64..=2u64 {
            let btn: Option<Retained<AnyObject>> =
                msg_send![&window, standardWindowButton: button_id];
            if let Some(b) = btn {
                let _: () = msg_send![&b, setHidden: true];
            }
        }
    }

    let viz_view: Retained<VizView> = unsafe {
        let view = VizView::alloc(mtm).set_ivars(());
        let view_frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(DEFAULT_W, DEFAULT_H));
        let view: Retained<VizView> = msg_send![super(view), initWithFrame: view_frame];
        let _: () = msg_send![&view, setAutoresizingMask: 18usize];
        view
    };
    window.setContentView(Some(&viz_view));
    unsafe {
        let _: bool = msg_send![&window, makeFirstResponder: &*viz_view];
    }

    let ctx =
        NSOpenGLContext::initWithFormat_shareContext(NSOpenGLContext::alloc(), &pixel_format, None)
            .expect("Failed to create NSOpenGLContext");

    #[allow(deprecated)]
    ctx.setView(Some(&viz_view), mtm);
    ctx.makeCurrentContext();

    let swap_interval: i32 = 1;
    unsafe {
        let _: () = msg_send![
            &ctx,
            setValues: &swap_interval,
            forParameter: NSOpenGLContextParameter::SwapInterval
        ];
    }

    gl::load_with(|s| unsafe {
        let sym = std::ffi::CString::new(s).unwrap();
        dlsym((-2isize) as *mut c_void, sym.as_ptr()) as *const c_void
    });

    let scale = window.backingScaleFactor();
    let pixel_w = (DEFAULT_W * scale) as u32;
    let pixel_h = (DEFAULT_H * scale) as u32;

    let preset_path = env!("PROJECTM_DATADIR").to_string() + "/presets/presets_stock";
    let viz = Visualizer::new(pixel_w, pixel_h, &preset_path).expect("Failed to create visualizer");

    let stock_count = viz.playlist_size() as usize;
    let user_preset_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("pip-milkdrop")
        .join("presets");
    viz.load_user_presets(&user_preset_dir.to_string_lossy());
    eprintln!(
        "[pip-milkdrop] Stock: {}, Total: {}, User dir: {}",
        stock_count,
        viz.playlist_size(),
        user_preset_dir.display()
    );

    ctx.update(mtm);
    eprintln!(
        "[pip-milkdrop] Initialized: {}x{} @ {scale}x = {pixel_w}x{pixel_h} px",
        DEFAULT_W as u32, DEFAULT_H as u32
    );

    let mut menubar = MenuBar::new();
    eprintln!("[pip-milkdrop] Menu bar created.");

    let mut gallery: Option<Gallery> = None;

    let mut capture = AudioCapture::new().expect("Failed to create audio capture");
    capture.start().expect("Failed to start audio capture");

    media::start_polling();
    eprintln!("[pip-milkdrop] Media polling started.");

    let mut ctrl = Controller::new();
    let mut config = config;

    let mut visible = debug.always_show;
    let mut user_dismissed_window = false;
    let mut debug_audio_time = 0.0f64;
    let mut last_status = Instant::now();
    let mut total_buffers = 0u64;
    let mut last_frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(0.0, 0.0));
    #[allow(unused_assignments)]
    let mut viz_pixel_w = pixel_w as i32;
    #[allow(unused_assignments)]
    let mut viz_pixel_h = pixel_h as i32;

    if debug.always_show {
        window.makeKeyAndOrderFront(None);
        unsafe {
            let _: bool = msg_send![&window, makeFirstResponder: &*viz_view];
        }
    } else {
        window.orderOut(None);
    }

    let distant_past = NSDate::distantPast();

    loop {
        let mut key_nav: i32 = 0;
        autoreleasepool(|_| loop {
            let event = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                Some(&distant_past),
                unsafe { NSDefaultRunLoopMode },
                true,
            );
            let Some(event) = event else { break };
            let event_type: usize = unsafe { msg_send![&event, type] };
            if event_type == 10 {
                // NSKeyDown. The visualizer is intentionally chrome-light, so handle
                // arrow navigation at the event loop level after the user clicks/focuses
                // the visualizer window.
                let event_window: *mut AnyObject = unsafe { msg_send![&event, window] };
                let key_window: *mut AnyObject = unsafe { msg_send![&app, keyWindow] };
                let viz_window = &*window as *const NSWindow as *mut AnyObject;
                if event_window == viz_window || key_window == viz_window {
                    let key_code: u16 = unsafe { msg_send![&event, keyCode] };
                    match key_code {
                        123 | 126 => {
                            key_nav = -1;
                            break;
                        }
                        124 | 125 => {
                            key_nav = 1;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            app.sendEvent(&event);
        });

        if key_nav < 0 {
            viz.select_previous();
            eprintln!("[pip-milkdrop] key nav: previous preset");
        } else if key_nav > 0 {
            viz.select_next();
            eprintln!("[pip-milkdrop] key nav: next preset");
        }

        let action = menubar.handle_pending_action(&mut config);
        if action == -1 {
            unsafe {
                let _: () = msg_send![&app, terminate: std::ptr::null::<AnyObject>()];
            }
            std::process::exit(0);
        }

        if action == menubar::TAG_NEXT as i32 || action == menubar::TAG_PREV as i32 {
            use config::ShuffleMode;
            let preset_names: Vec<String> = (0..viz.playlist_size())
                .map(|i| viz.preset_name(i))
                .collect();
            let fav_indices: Vec<u32> = preset_names
                .iter()
                .enumerate()
                .filter(|(_, name)| config.favorites.contains(*name))
                .map(|(i, _)| i as u32)
                .collect();
            let all_count = viz.playlist_size();
            match config.shuffle_mode {
                ShuffleMode::Off => {
                    if action == menubar::TAG_NEXT as i32 {
                        viz.select_next();
                    } else {
                        viz.select_previous();
                    }
                }
                ShuffleMode::All => {
                    let all: Vec<u32> = (0..all_count).collect();
                    viz.select_random_from(&all);
                }
                ShuffleMode::Favorites => {
                    viz.select_random_from(&fav_indices);
                }
            }
        }

        if action == menubar::TAG_TOGGLE_FAV as i32 {
            let name = viz.preset_name(viz.selected_preset_index());
            if config.favorites.contains(&name) {
                config.favorites.remove(&name);
            } else {
                config.favorites.insert(name);
            }
            config.save();
        }
        if action == menubar::TAG_BROWSE as i32 {
            if gallery.is_none() {
                let names: Vec<String> = (0..viz.playlist_size())
                    .map(|i| viz.preset_name(i))
                    .collect();
                gallery = Some(Gallery::new(
                    &names,
                    stock_count,
                    &config.favorites,
                    viz.selected_preset_index() as usize,
                    mtm,
                ));
            }
            if let Some(ref mut g) = gallery {
                g.show();
            }
        }

        if let Some(ref mut g) = gallery {
            g.check_closed();

            let ga = gallery::GALLERY_ACTION.swap(0, Ordering::Relaxed);
            match ga {
                v if v >= gallery::GA_FAV_BASE => {
                    let idx = (v - gallery::GA_FAV_BASE) as usize;
                    let name = viz.preset_name(idx as u32);
                    if config.favorites.contains(&name) {
                        config.favorites.remove(&name);
                    } else {
                        config.favorites.insert(name);
                    }
                    config.save();
                    g.toggle_favorite(idx);
                }
                v if v >= gallery::GA_SELECT_BASE => {
                    let idx = (v - gallery::GA_SELECT_BASE) as u32;
                    viz.select_preset(idx);
                    g.update_active(idx as usize);
                }
                v if v >= gallery::GA_SECTION_BASE && v < gallery::GA_SELECT_BASE => {
                    let section_idx = (v - gallery::GA_SECTION_BASE) as usize;
                    g.toggle_section(section_idx);
                }
                1 => {
                    g.apply_filter();
                }
                2 => {
                    g.clear_filter();
                }
                3 => {
                    g.set_tab_all();
                }
                4 => {
                    g.set_tab_favorites();
                }
                _ => {}
            }
        }

        if audio::DEVICE_CHANGED.swap(false, Ordering::Relaxed) {
            eprintln!("[pip-milkdrop] Audio device changed, restarting capture...");
            if let Err(e) = capture.restart() {
                eprintln!("[pip-milkdrop] Failed to restart audio capture: {e}");
            }
        }

        let frame = window.frame();
        if frame.size.width != last_frame.size.width || frame.size.height != last_frame.size.height
        {
            let view_bounds: CGRect = unsafe { msg_send![&viz_view, bounds] };
            let scale = window.backingScaleFactor();
            viz_pixel_w = (view_bounds.size.width * scale) as i32;
            viz_pixel_h = (view_bounds.size.height * scale) as i32;
            viz.reset_gl(viz_pixel_w, viz_pixel_h);
            ctx.update(mtm);
            last_frame = frame;
        }

        let mut latest_rms = 0.0f32;
        let mut audio_buffers = 0u32;
        while let Ok(samples) = capture.rx.try_recv() {
            latest_rms = compute_rms(&samples);
            audio_buffers += 1;
            if visible && !debug.simulated_audio {
                viz.add_pcm_float_stereo(&samples);
            }
        }
        if debug.simulated_audio {
            let samples = generate_debug_audio(&mut debug_audio_time);
            latest_rms = compute_rms(&samples);
            audio_buffers += 1;
            viz.add_pcm_float_stereo(&samples);
        }
        total_buffers += audio_buffers as u64;

        if last_status.elapsed() >= Duration::from_secs(3) {
            let idx = viz.selected_preset_index();
            let name = viz.preset_name(idx);
            eprintln!(
                "[pip-milkdrop] rms={latest_rms:.4} media={} vis={visible} buf={total_buffers} preset={name}",
                media::is_media_playing()
            );
            menubar.update_state(&config, &name);
            last_status = Instant::now();
        }

        let media_playing = if debug.simulated_audio {
            true
        } else {
            media::is_media_playing()
        };
        let _changed = ctrl.update(latest_rms, media_playing, &config);

        let gallery_open = gallery.as_ref().map_or(false, |g| g.is_open());
        let hover = gallery::GALLERY_HOVER.load(Ordering::Relaxed);
        let hover_active = gallery_open && hover >= 0;

        let activity_present = latest_rms >= config.rms_threshold() || media_playing;
        if !activity_present {
            user_dismissed_window = false;
        }

        let window_is_visible: bool = unsafe { msg_send![&window, isVisible] };
        if visible && !window_is_visible {
            // User closed/minimized the floating window. Do not immediately fight them by
            // reopening it while the same audio burst is still active.
            visible = false;
            user_dismissed_window = true;
        }

        let should_show = (debug.always_show
            || match ctrl.visibility {
                Visibility::Visible => true,
                Visibility::Hidden => false,
            })
            && (!user_dismissed_window || debug.always_show);

        if should_show && !visible {
            window.makeKeyAndOrderFront(None);
            unsafe {
                let _: bool = msg_send![&window, makeFirstResponder: &*viz_view];
            }
            visible = true;
        } else if !should_show && visible {
            window.orderOut(None);
            visible = false;
        }

        if visible || gallery_open {
            ctx.makeCurrentContext();
        }

        if let Some(ref mut g) = gallery {
            if g.is_open() {
                // Keep chrome/grid layout in sync during live resize even when the mouse
                // is hovering a card and thumbnail preview rendering is paused.
                g.sync_layout_to_bounds();
                g.update_hover(hover);
            }
            if g.is_open() && !hover_active {
                g.tick(&viz);
            }
        }

        if hover_active {
            let hover_idx = hover as usize;
            if let Some(ref mut g) = gallery {
                if let Some(image) = g.render_hover_frame(&viz, hover_idx) {
                    g.set_card_image(hover_idx, &image);
                }
            }
        }

        if visible && !hover_active {
            viz.render_frame();
            ctx.flushBuffer();
        } else if !gallery_open && !hover_active {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
