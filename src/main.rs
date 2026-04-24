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
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};

use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::AnyObject;
use objc2::{class, define_class, msg_send, AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSEvent, NSEventMask,
    NSFloatingWindowLevel, NSOpenGLContext, NSOpenGLContextParameter,
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

const SCROLL_TAG: i32 = 1;
const DRAG_START_TAG: i32 = 2;
const DRAG_MOVE_TAG: i32 = 3;
const DRAG_END_TAG: i32 = 4;

static PENDING_VIEW_EVENT: AtomicI32 = AtomicI32::new(0);
static PENDING_DX: AtomicI32 = AtomicI32::new(0);
static PENDING_DY: AtomicI32 = AtomicI32::new(0);

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

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            PENDING_VIEW_EVENT.store(DRAG_START_TAG, Ordering::Relaxed);
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            PENDING_VIEW_EVENT.store(DRAG_END_TAG, Ordering::Relaxed);
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let dx = event.deltaX() as i32;
            let dy = event.deltaY() as i32;
            PENDING_DX.store(dx, Ordering::Relaxed);
            PENDING_DY.store(dy, Ordering::Relaxed);
            PENDING_VIEW_EVENT.store(DRAG_MOVE_TAG, Ordering::Relaxed);
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            let dy = event.scrollingDeltaY() as i32;
            PENDING_DY.store(dy, Ordering::Relaxed);
            PENDING_VIEW_EVENT.store(SCROLL_TAG, Ordering::Relaxed);
        }
    }
);

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            window_rect,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };

    window.setLevel(NSFloatingWindowLevel);
    window.setOpaque(true);
    window.setHasShadow(true);
    unsafe {
        let _: () = msg_send![&window, setReleasedWhenClosed: false];
        let _: () = msg_send![&window, setIgnoresMouseEvents: false];
        let _: () = msg_send![&window, setMovableByWindowBackground: false];
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
    let viz = Visualizer::new(pixel_w, pixel_h, preset_path)?;

    ctx.update(mtm);
    eprintln!(
        "[pip-milkdrop] Initialized: {}x{} @ {scale}x = {pixel_w}x{pixel_h} px",
        DEFAULT_W as u32, DEFAULT_H as u32
    );

    let mut menubar = MenuBar::new();
    menubar.populate_presets(&viz);
    eprintln!("[pip-milkdrop] Menu bar created.");

    let mut gallery: Option<Gallery> = None;

    let mut capture = AudioCapture::new()?;
    capture.start()?;

    media::start_polling();
    eprintln!("[pip-milkdrop] Media polling started.");

    let mut ctrl = Controller::new();
    let mut config = config;

    let mut visible = false;
    let mut dragging = false;
    let mut last_status = Instant::now();
    let mut total_buffers = 0u64;
    let mut need_resize = false;
    #[allow(unused_assignments)]
    let mut viz_pixel_w = pixel_w as i32;
    #[allow(unused_assignments)]
    let mut viz_pixel_h = pixel_h as i32;

    window.orderOut(None);

    let distant_past = NSDate::distantPast();

    let mut running = true;
    while running {
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

        loop {
            let tag = PENDING_VIEW_EVENT.swap(0, Ordering::Relaxed);
            if tag == 0 {
                break;
            }
            match tag {
                DRAG_START_TAG => {
                    dragging = true;
                }
                DRAG_END_TAG => {
                    dragging = false;
                }
                DRAG_MOVE_TAG => {
                    if dragging {
                        let dx = PENDING_DX.load(Ordering::Relaxed) as f64;
                        let dy = PENDING_DY.load(Ordering::Relaxed) as f64;
                        let frame = window.frame();
                        let new_origin =
                            CGPoint::new(frame.origin.x + dx, frame.origin.y - dy);
                        window.setFrameOrigin(new_origin);
                    }
                }
                t if t == SCROLL_TAG => {
                    let dy = PENDING_DY.load(Ordering::Relaxed) as f64;
                    let scale = if dy > 0.0 { 1.15f64 } else { 1.0 / 1.15 };
                    let frame = window.frame();
                    let nw = (frame.size.width * scale).clamp(100.0, 800.0);
                    let nh = (frame.size.height * scale).clamp(100.0, 800.0);
                    let new_x = frame.origin.x + (frame.size.width - nw) / 2.0;
                    let new_y = frame.origin.y + (frame.size.height - nh) / 2.0;
                    let new_frame =
                        CGRect::new(CGPoint::new(new_x, new_y), CGSize::new(nw, nh));
                    window.setFrame_display(new_frame, true);
                    need_resize = true;
                }
                _ => {}
            }
        }

        let action = menubar.handle_pending_action(&mut config, &viz);
        if action == -1 {
            running = false;
            continue;
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

        if need_resize {
            let view_bounds: CGRect = unsafe { msg_send![&viz_view, bounds] };
            let scale = window.backingScaleFactor();
            viz_pixel_w = (view_bounds.size.width * scale) as i32;
            viz_pixel_h = (view_bounds.size.height * scale) as i32;
            viz.reset_gl(viz_pixel_w, viz_pixel_h);
            ctx.update(mtm);
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

        // Hover preview: render hovered preset into card's image view
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

    capture.stop();
    Ok(())
}
