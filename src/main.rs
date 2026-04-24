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
    NSFloatingWindowLevel,
    NSOpenGLContext, NSOpenGLContextParameter,
    NSOpenGLPFAAlphaSize, NSOpenGLPFADepthSize, NSOpenGLPFADoubleBuffer, NSOpenGLPFAColorSize,
    NSOpenGLPFAOpenGLProfile, NSOpenGLPixelFormat, NSOpenGLProfileVersion3_2Core, NSView,
    NSWindow, NSWindowStyleMask,
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
    }
);

fn main() {
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
        NSOpenGLProfileVersion3_2Core,
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
    window.setMinSize(CGSize::new(100.0, 100.0));
    window.setMaxSize(CGSize::new(800.0, 800.0));
    unsafe {
        let _: () = msg_send![&window, setReleasedWhenClosed: false];
        let _: () = msg_send![&window, setTitlebarAppearsTransparent: true];
        let _: () = msg_send![&window, setTitleVisibility: 1u64]; // NSWindowTitleHidden
        let _: () = msg_send![&window, setMovableByWindowBackground: true];
        let _: () = msg_send![&window, setShowsResizeIndicator: false];
        let btn: Option<Retained<AnyObject>> = msg_send![&window, standardWindowButton: 0u64]; // NSWindowCloseButton
        if let Some(b) = btn {
            let _: () = msg_send![&b, setHidden: true];
        }
        let btn: Option<Retained<AnyObject>> = msg_send![&window, standardWindowButton: 1u64]; // NSWindowMiniaturizeButton
        if let Some(b) = btn {
            let _: () = msg_send![&b, setHidden: true];
        }
        let btn: Option<Retained<AnyObject>> = msg_send![&window, standardWindowButton: 2u64]; // NSWindowZoomButton
        if let Some(b) = btn {
            let _: () = msg_send![&b, setHidden: true];
        }
    }

    let viz_view: Retained<VizView> = unsafe {
        let view = VizView::alloc(mtm).set_ivars(());
        let view: Retained<VizView> = msg_send![super(view), initWithFrame: window_rect];
        view
    };
    window.setContentView(Some(&viz_view));

    let ctx = NSOpenGLContext::initWithFormat_shareContext(
        NSOpenGLContext::alloc(),
        &pixel_format,
        None,
    )
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

    let preset_path = "/opt/homebrew/share/projectM/presets/presets_stock";
    let viz = Visualizer::new(pixel_w, pixel_h, preset_path).expect("Failed to create visualizer");

    ctx.update(mtm);
    eprintln!(
        "[pip-milkdrop] Initialized: {}x{} @ {scale}x = {pixel_w}x{pixel_h} px",
        DEFAULT_W as u32, DEFAULT_H as u32
    );

    let mut menubar = MenuBar::new();
    menubar.populate_presets(&viz);
    eprintln!("[pip-milkdrop] Menu bar created.");

    let mut gallery: Option<Gallery> = None;

    let mut capture = AudioCapture::new().expect("Failed to create audio capture");
    capture.start().expect("Failed to start audio capture");

    media::start_polling();
    eprintln!("[pip-milkdrop] Media polling started.");

    let mut ctrl = Controller::new();
    let mut config = config;

    let mut visible = false;
    let mut last_status = Instant::now();
    let mut total_buffers = 0u64;
    let mut last_frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(0.0, 0.0));
    #[allow(unused_assignments)]
    let mut viz_pixel_w = pixel_w as i32;
    #[allow(unused_assignments)]
    let mut viz_pixel_h = pixel_h as i32;

    window.orderOut(None);

    let distant_past = NSDate::distantPast();

    loop {
        autoreleasepool(|_| {
            loop {
                let event = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                    NSEventMask::Any,
                    Some(&distant_past),
                    unsafe { NSDefaultRunLoopMode },
                    true,
                );
                let Some(event) = event else { break };
                app.sendEvent(&event);
            }
        });

        let action = menubar.handle_pending_action(&mut config, &viz);
        if action == -1 {
            unsafe {
                let _: () = msg_send![&app, terminate: std::ptr::null::<AnyObject>()];
            }
            std::process::exit(0);
        }
        if action == menubar::TAG_BROWSE as i32 {
            if gallery.is_none() {
                let names: Vec<String> = (0..viz.playlist_size()).map(|i| viz.preset_name(i)).collect();
                gallery = Some(Gallery::new(
                    &names,
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
                    menubar.rebuild_favorites(&config);
                }
                v if v >= gallery::GA_SELECT_BASE => {
                    let idx = (v - gallery::GA_SELECT_BASE) as u32;
                    viz.select_preset(idx);
                    g.update_active(idx as usize);
                }
                1 => {
                    g.apply_filter();
                }
                2 => {
                    g.clear_filter();
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
        if frame.size.width != last_frame.size.width || frame.size.height != last_frame.size.height {
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
            if visible {
                viz.add_pcm_float_stereo(&samples);
            }
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

        let media_playing = media::is_media_playing();
        let _changed = ctrl.update(latest_rms, media_playing, &config);

        let gallery_open = gallery.as_ref().map_or(false, |g| g.is_open());
        let hover = gallery::GALLERY_HOVER.load(Ordering::Relaxed);
        let hover_active = gallery_open && hover >= 0;

        let should_show = match ctrl.visibility {
            Visibility::Visible => true,
            Visibility::Hidden => false,
        };

        if should_show && !visible {
            window.makeKeyAndOrderFront(None);
            visible = true;
        } else if !should_show && visible {
            window.orderOut(None);
            visible = false;
        }

        if visible || gallery_open {
            ctx.makeCurrentContext();
        }

        if let Some(ref mut g) = gallery {
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
