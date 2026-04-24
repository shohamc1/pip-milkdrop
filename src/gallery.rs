use std::collections::HashSet;
use std::sync::atomic::{AtomicI32, Ordering};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{class, define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSView, NSWindow,
    NSWindowStyleMask,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::NSString;

use crate::visualizer::Visualizer;

pub static GALLERY_ACTION: AtomicI32 = AtomicI32::new(0);
pub static GALLERY_HOVER: AtomicI32 = AtomicI32::new(-1);

pub const GA_SEARCH: i32 = 1;
pub const GA_CLEAR: i32 = 2;
pub const GA_SELECT_BASE: i32 = 1000;
pub const GA_FAV_BASE: i32 = 5000;

const PREVIEW_W: usize = 300;
const PREVIEW_H: usize = 300;
const CARD_W: f64 = 160.0;
const CARD_H: f64 = 210.0;
const IMG_SIZE: f64 = 150.0;
const PAD: f64 = 8.0;
const COLS: usize = 5;
const WARMUP_INITIAL: usize = 8;
const FRAMES_PER_TICK: usize = 2;

define_class!(
    #[unsafe(super(NSObject))]
    struct GalleryHandler;

    impl GalleryHandler {
        #[unsafe(method(favClicked:))]
        fn fav_clicked(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let tag: isize = unsafe { msg_send![sender, tag] };
            GALLERY_ACTION.store(GA_FAV_BASE + tag as i32, Ordering::Relaxed);
        }

        #[unsafe(method(searchClicked:))]
        fn search_clicked(&self, _sender: Option<&AnyObject>) {
            GALLERY_ACTION.store(GA_SEARCH, Ordering::Relaxed);
        }

        #[unsafe(method(clearClicked:))]
        fn clear_clicked(&self, _sender: Option<&AnyObject>) {
            GALLERY_ACTION.store(GA_CLEAR, Ordering::Relaxed);
        }
    }
);

impl GalleryHandler {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

define_class!(
    #[unsafe(super(NSView))]
    #[ivars = (usize,)]
    struct CardView;

    impl CardView {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &objc2_app_kit::NSEvent) {
            let idx = self.ivars().0;
            GALLERY_ACTION.store(GA_SELECT_BASE + idx as i32, Ordering::Relaxed);
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &objc2_app_kit::NSEvent) {
            let idx = self.ivars().0;
            GALLERY_HOVER.store(idx as i32, Ordering::Relaxed);
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &objc2_app_kit::NSEvent) {
            GALLERY_HOVER.store(-1, Ordering::Relaxed);
        }
    }
);

fn generate_simulated_audio(time: &mut f64) -> Vec<f32> {
    let n = 512;
    let dt = n as f64 / (2.0 * 44100.0);
    let mut pcm = vec![0.0f32; n];
    let pi = std::f64::consts::PI;
    for i in 0..(n / 2) {
        let t = *time + (i as f64) / 44100.0;
        let beat = (1.0 + (t * 4.0 * pi).sin()) * 0.5;
        let kick_env = (1.0 - ((t * 4.0) % 1.0)).max(0.0).powf(2.0);
        let kick = (t * 2.0 * pi * 80.0).sin() * kick_env * 0.6;
        let bass = (t * 2.0 * pi * 120.0).sin() * 0.35 * beat;
        let bass2 = (t * 2.0 * pi * 60.0).sin() * 0.25 * (1.0 - beat);
        let mid = (t * 2.0 * pi * 440.0).sin() * 0.2 * (1.0 - beat);
        let mid2 = (t * 2.0 * pi * 660.0).sin() * 0.15 * ((t * 2.0).sin() * 0.5 + 0.5);
        let high = (t * 2.0 * pi * 3000.0).sin() * 0.12 * ((t * 8.0).sin() * 0.5 + 0.5);
        let noise = ((t * 12345.6789).sin() * 43758.5453).sin() * 0.08;
        let sample = (kick + bass + bass2 + mid + mid2 + high + noise) as f32;
        let clamped = sample.clamp(-1.0, 1.0);
        pcm[i * 2] = clamped;
        pcm[i * 2 + 1] = clamped;
    }
    *time += dt;
    pcm
}

unsafe fn create_nsimage_from_pixels(
    pixels: &[u8],
    w: usize,
    h: usize,
) -> Option<Retained<AnyObject>> {
    let color_space = NSString::from_str("NSDeviceRGBColorSpace");
    let rep: *mut AnyObject = msg_send![class!(NSBitmapImageRep), alloc];
    let rep: *mut AnyObject = msg_send![rep,
        initWithBitmapDataPlanes: std::ptr::null_mut::<*mut u8>(),
        pixelsWide: w as isize,
        pixelsHigh: h as isize,
        bitsPerSample: 8isize,
        samplesPerPixel: 4isize,
        hasAlpha: true,
        isPlanar: false,
        colorSpaceName: &*color_space,
        bytesPerRow: (w * 4) as isize,
        bitsPerPixel: 32isize
    ];
    let rep = Retained::from_raw(rep)?;
    let data_ptr: *mut u8 = msg_send![&*rep, bitmapData];
    std::ptr::copy_nonoverlapping(pixels.as_ptr(), data_ptr, pixels.len());

    let image: *mut AnyObject = msg_send![class!(NSImage), alloc];
    let image: *mut AnyObject = msg_send![image, initWithSize: CGSize::new(w as f64, h as f64)];
    let image = Retained::from_raw(image)?;
    let () = msg_send![&*image, addRepresentation: &*rep];
    Some(image)
}

fn capture_gl_image() -> Option<Retained<AnyObject>> {
    let mut viewport = [0i32; 4];
    unsafe { gl::GetIntegerv(gl::VIEWPORT, viewport.as_mut_ptr()) };
    let fb_w = viewport[2] as usize;
    let fb_h = viewport[3] as usize;
    let read_w = fb_w.min(PREVIEW_W);
    let read_h = fb_h.min(PREVIEW_H);

    let mut pixels = vec![0u8; read_w * read_h * 4];
    unsafe {
        gl::ReadPixels(
            0,
            0,
            read_w as i32,
            read_h as i32,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            pixels.as_mut_ptr() as *mut _,
        );
    }

    let all_zero = pixels.iter().all(|&b| b == 0);
    if all_zero {
        return None;
    }

    let row_bytes = read_w * 4;
    let mut flipped = vec![0u8; pixels.len()];
    for y in 0..read_h {
        let src = y * row_bytes;
        let dst = (read_h - 1 - y) * row_bytes;
        flipped[dst..dst + row_bytes].copy_from_slice(&pixels[src..src + row_bytes]);
    }

    unsafe { create_nsimage_from_pixels(&flipped, read_w, read_h) }
}

struct Card {
    view: Retained<CardView>,
    image_view: Retained<AnyObject>,
    #[allow(dead_code)]
    name_label: Retained<AnyObject>,
    fav_button: Retained<AnyObject>,
}

pub struct Gallery {
    window: Retained<NSWindow>,
    #[allow(dead_code)]
    scroll_view: Retained<AnyObject>,
    document_view: Retained<AnyObject>,
    search_field: Retained<AnyObject>,
    #[allow(dead_code)]
    handler: Retained<GalleryHandler>,
    cards: Vec<Card>,
    preview_images: Vec<Option<Retained<AnyObject>>>,
    all_presets: Vec<String>,
    visible_indices: Vec<usize>,
    active_index: usize,
    favorites: HashSet<String>,
    filter: Option<String>,
    is_open: bool,

    sim_time: f64,
    current_preview: Option<usize>,
    preview_frames: usize,
    saved_preset: u32,
    initial_queue: Vec<usize>,
    initial_queued: HashSet<usize>,
}

impl Gallery {
    pub fn new(
        presets: &[String],
        favorites: &HashSet<String>,
        active_index: usize,
        mtm: MainThreadMarker,
    ) -> Self {
        let handler = GalleryHandler::new();
        let handler_ref: &AnyObject = handler.as_ref();
        let fav_sel = sel!(favClicked:);
        let search_sel = sel!(searchClicked:);
        let clear_sel = sel!(clearClicked:);

        let win_w = 900.0;
        let win_h = 700.0;
        let screen_frame: CGRect = unsafe {
            let screen: Option<Retained<AnyObject>> = msg_send![class!(NSScreen), mainScreen];
            let screen = screen.expect("No screen");
            msg_send![&*screen, visibleFrame]
        };
        let win_x = screen_frame.origin.x + (screen_frame.size.width - win_w) / 2.0;
        let win_y = screen_frame.origin.y + (screen_frame.size.height - win_h) / 2.0;
        let win_rect = CGRect::new(CGPoint::new(win_x, win_y), CGSize::new(win_w, win_h));

        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                win_rect,
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Resizable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe {
            let () = msg_send![&window, setTitle: &*NSString::from_str("pip-milkdrop \u{2014} Browse Presets")];
            let () = msg_send![&window, setReleasedWhenClosed: false];
            let () = msg_send![&window, setMinSize: CGSize::new(400.0, 300.0)];
        }

        let content_rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(win_w, win_h));
        let content_view = NSView::initWithFrame(NSView::alloc(mtm), content_rect);
        window.setContentView(Some(&content_view));

        let search_h = 28.0;
        let search_y = win_h - search_h - PAD;
        let search_field: Retained<AnyObject> = unsafe {
            let tf: *mut AnyObject = msg_send![class!(NSSearchField), alloc];
            let tf: *mut AnyObject = msg_send![tf, initWithFrame: CGRect::new(
                CGPoint::new(PAD, search_y),
                CGSize::new(win_w - 200.0, search_h),
            )];
            let tf = Retained::from_raw(tf).unwrap();
            let () = msg_send![&*tf, setEditable: true];
            let () = msg_send![&*tf, setPlaceholderString: &*NSString::from_str("Filter presets...")];
            let () = msg_send![&*tf, setAutoresizingMask: 10usize];
            tf
        };
        unsafe {
            let () = msg_send![&*content_view, addSubview: &*search_field];
        }

        let search_btn: Retained<AnyObject> = unsafe {
            let btn: *mut AnyObject = msg_send![class!(NSButton), alloc];
            let btn: *mut AnyObject = msg_send![btn, initWithFrame: CGRect::new(
                CGPoint::new(win_w - 180.0, search_y),
                CGSize::new(80.0, search_h),
            )];
            let btn = Retained::from_raw(btn).unwrap();
            let () = msg_send![&*btn, setTitle: &*NSString::from_str("Search")];
            let () = msg_send![&*btn, setTarget: handler_ref];
            let () = msg_send![&*btn, setAction: search_sel];
            let () = msg_send![&*btn, setBezelStyle: 1isize];
            let () = msg_send![&*btn, setAutoresizingMask: 9usize];
            btn
        };
        unsafe {
            let () = msg_send![&*content_view, addSubview: &*search_btn];
        }

        let clear_btn: Retained<AnyObject> = unsafe {
            let btn: *mut AnyObject = msg_send![class!(NSButton), alloc];
            let btn: *mut AnyObject = msg_send![btn, initWithFrame: CGRect::new(
                CGPoint::new(win_w - 90.0, search_y),
                CGSize::new(80.0, search_h),
            )];
            let btn = Retained::from_raw(btn).unwrap();
            let () = msg_send![&*btn, setTitle: &*NSString::from_str("Clear")];
            let () = msg_send![&*btn, setTarget: handler_ref];
            let () = msg_send![&*btn, setAction: clear_sel];
            let () = msg_send![&*btn, setBezelStyle: 1isize];
            let () = msg_send![&*btn, setAutoresizingMask: 9usize];
            btn
        };
        unsafe {
            let () = msg_send![&*content_view, addSubview: &*clear_btn];
        }

        let grid_h = search_y - PAD;
        let scroll_rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(win_w, grid_h));
        let scroll_view: Retained<AnyObject> = unsafe {
            let sv: *mut AnyObject = msg_send![class!(NSScrollView), alloc];
            let sv: *mut AnyObject = msg_send![sv, initWithFrame: scroll_rect];
            let sv = Retained::from_raw(sv).unwrap();
            let () = msg_send![&*sv, setHasVerticalScroller: true];
            let () = msg_send![&*sv, setAutohidesScrollers: true];
            let () = msg_send![&*sv, setAutoresizingMask: 18usize];
            sv
        };
        unsafe {
            let () = msg_send![&*content_view, addSubview: &*scroll_view];
        }

        let total = presets.len();
        let rows = (total + COLS - 1) / COLS;
        let doc_h = PAD + rows as f64 * (CARD_H + PAD);
        let doc_w = COLS as f64 * (CARD_W + PAD) + PAD;
        let doc_rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(doc_w, doc_h));
        let document_view: Retained<AnyObject> = unsafe {
            let dv: *mut AnyObject = msg_send![class!(NSView), alloc];
            let dv: *mut AnyObject = msg_send![dv, initWithFrame: doc_rect];
            Retained::from_raw(dv).unwrap()
        };
        unsafe {
            let () = msg_send![&*scroll_view, setDocumentView: &*document_view];
        }

        let small_font: Retained<AnyObject> =
            unsafe { msg_send![class!(NSFont), systemFontOfSize: 11.0] };
        let star_font: Retained<AnyObject> =
            unsafe { msg_send![class!(NSFont), systemFontOfSize: 16.0] };
        let white_color: *mut AnyObject = unsafe { msg_send![class!(NSColor), whiteColor] };

        let mut cards = Vec::with_capacity(total);
        for (i, name) in presets.iter().enumerate() {
            let row = i / COLS;
            let col = i % COLS;
            let x = PAD + col as f64 * (CARD_W + PAD);
            let y = doc_h - PAD - row as f64 * (CARD_H + PAD) - CARD_H;
            let card_rect = CGRect::new(CGPoint::new(x, y), CGSize::new(CARD_W, CARD_H));

            let card_view: Retained<CardView> = unsafe {
                let view = CardView::alloc(mtm).set_ivars((i,));
                msg_send![super(view), initWithFrame: card_rect]
            };
            unsafe {
                let () = msg_send![&*card_view, setWantsLayer: true];
                let layer: *mut AnyObject = msg_send![&*card_view, layer];
                let () = msg_send![layer, setCornerRadius: 6.0];
                let () = msg_send![layer, setMasksToBounds: true];
                let bg: *mut AnyObject = msg_send![class!(NSColor), colorWithCalibratedRed: 0.12, green: 0.12, blue: 0.14, alpha: 1.0];
                let cg: *mut AnyObject = msg_send![&*bg, CGColor];
                let () = msg_send![layer, setBackgroundColor: cg];

                let bounds: CGRect = msg_send![&*card_view, bounds];
                let tracking: *mut AnyObject = msg_send![class!(NSTrackingArea), alloc];
                let tracking: *mut AnyObject = msg_send![tracking,
                    initWithRect: bounds
                    options: 129usize
                    owner: &*card_view
                    userInfo: std::ptr::null_mut::<AnyObject>()
                ];
                if let Some(tracking) = Retained::from_raw(tracking) {
                    let () = msg_send![&*card_view, addTrackingArea: &*tracking];
                }
            }

            let img_rect = CGRect::new(
                CGPoint::new((CARD_W - IMG_SIZE) / 2.0, 40.0),
                CGSize::new(IMG_SIZE, IMG_SIZE),
            );
            let image_view: Retained<AnyObject> = unsafe {
                let iv: *mut AnyObject = msg_send![class!(NSImageView), alloc];
                let iv: *mut AnyObject = msg_send![iv, initWithFrame: img_rect];
                let iv = Retained::from_raw(iv).unwrap();
                let () = msg_send![&*iv, setImageScaling: 1isize];
                let () = msg_send![&*iv, setEditable: false];
                iv
            };
            unsafe {
                let () = msg_send![&*card_view, addSubview: &*image_view];
            }

            let label_rect = CGRect::new(CGPoint::new(4.0, 4.0), CGSize::new(CARD_W - 8.0, 28.0));
            let name_label: Retained<AnyObject> = unsafe {
                let tf: *mut AnyObject = msg_send![class!(NSTextField), alloc];
                let tf: *mut AnyObject = msg_send![tf, initWithFrame: label_rect];
                let tf = Retained::from_raw(tf).unwrap();
                let () = msg_send![&*tf, setEditable: false];
                let () = msg_send![&*tf, setSelectable: false];
                let () = msg_send![&*tf, setBezeled: false];
                let () = msg_send![&*tf, setDrawsBackground: false];
                let () = msg_send![&*tf, setFont: &*small_font];
                let () = msg_send![&*tf, setTextColor: white_color];
                let () = msg_send![&*tf, setAlignment: 0isize];
                let () = msg_send![&*tf, setLineBreakMode: 0isize];
                let clean = name.strip_suffix(".milk").unwrap_or(name);
                let display_name = clean.to_string();
                let () = msg_send![&*tf, setStringValue: &*NSString::from_str(&display_name)];
                tf
            };
            unsafe {
                let () = msg_send![&*card_view, addSubview: &*name_label];
            }

            let fav_rect = CGRect::new(CGPoint::new(CARD_W - 26.0, IMG_SIZE + 40.0 - 24.0), CGSize::new(24.0, 24.0));
            let is_fav = favorites.contains(name);
            let fav_button: Retained<AnyObject> = unsafe {
                let btn: *mut AnyObject = msg_send![class!(NSButton), alloc];
                let btn: *mut AnyObject = msg_send![btn, initWithFrame: fav_rect];
                let btn = Retained::from_raw(btn).unwrap();
                let () = msg_send![&*btn, setBordered: false];
                let () = msg_send![&*btn, setButtonType: 5isize];
                let () = msg_send![&*btn, setTitle: &*NSString::from_str(if is_fav { "\u{2605}" } else { "\u{2606}" })];
                let () = msg_send![&*btn, setFont: &*star_font];
                let () = msg_send![&*btn, setTag: i as isize];
                let () = msg_send![&*btn, setTarget: handler_ref];
                let () = msg_send![&*btn, setAction: fav_sel];
                btn
            };
            unsafe {
                let () = msg_send![&*card_view, addSubview: &*fav_button];
            }

            cards.push(Card {
                view: card_view,
                image_view,
                name_label,
                fav_button,
            });
        }

        for card in &cards {
            unsafe {
                let () = msg_send![&*document_view, addSubview: &*card.view];
            }
        }

        let preview_images = vec![None; total];

        let mut initial_queue = Vec::new();
        let mut initial_queued = HashSet::new();
        for idx in 0..total {
            initial_queued.insert(idx);
            initial_queue.push(idx);
        }

        let visible_indices: Vec<usize> = (0..total).collect();

        let mut gallery = Self {
            window,
            scroll_view,
            document_view,
            search_field,
            handler,
            cards,
            preview_images,
            all_presets: presets.to_vec(),
            visible_indices,
            active_index,
            favorites: favorites.clone(),
            filter: None,
            is_open: false,
            sim_time: 0.0,
            current_preview: None,
            preview_frames: 0,
            saved_preset: 0,
            initial_queue,
            initial_queued,
        };
        gallery.update_active(active_index);
        gallery
    }

    pub fn show(&mut self) {
        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);
        unsafe {
            let () = msg_send![&app, setActivationPolicy: NSApplicationActivationPolicy::Regular];
            let () = msg_send![&app, activateIgnoringOtherApps: true];
        }
        self.window.makeKeyAndOrderFront(None);
        self.is_open = true;

        unsafe {
            let clip_view: Retained<AnyObject> = msg_send![&*self.scroll_view, contentView];
            let clip_bounds: CGRect = msg_send![&*clip_view, bounds];
            let doc_frame: CGRect = msg_send![&*self.document_view, frame];
            let target_y = (doc_frame.size.height - clip_bounds.size.height).max(0.0);
            let () = msg_send![&*clip_view, scrollToPoint: CGPoint::new(0.0, target_y)];
            let () = msg_send![&*self.scroll_view, reflectScrolledClipView: &*clip_view];
        }
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn check_closed(&mut self) {
        if !self.is_open {
            return;
        }
        let visible: bool = unsafe { msg_send![&self.window, isVisible] };
        if !visible {
            self.is_open = false;
            let mtm = MainThreadMarker::new().unwrap();
            let app = NSApplication::sharedApplication(mtm);
            unsafe {
                let () = msg_send![&app, setActivationPolicy: NSApplicationActivationPolicy::Accessory];
            }
        }
    }

    pub fn tick(&mut self, viz: &Visualizer) {
        if !self.is_open {
            return;
        }

        if self.current_preview.is_none() {
            if self.initial_queue.is_empty() {
                return;
            }
            let preset_idx = self.initial_queue.remove(0);
            self.initial_queued.remove(&preset_idx);

            self.current_preview = Some(preset_idx);
            self.preview_frames = 0;
            self.saved_preset = viz.selected_preset_index();
            viz.select_preset(preset_idx as u32);
        }

        for _ in 0..FRAMES_PER_TICK {
            let pcm = generate_simulated_audio(&mut self.sim_time);
            viz.add_pcm_float_stereo(&pcm);
            viz.render_frame();
            self.preview_frames += 1;
        }

        if self.preview_frames >= WARMUP_INITIAL {
            let idx = self.current_preview.unwrap();

            if let Some(image) = capture_gl_image() {
                if idx < self.preview_images.len() {
                    self.preview_images[idx] = Some(image.clone());
                    if idx < self.cards.len() {
                        unsafe {
                            let () = msg_send![&*self.cards[idx].image_view, setImage: &*image];
                        }
                    }
                }
            }

            viz.select_preset(self.saved_preset);
            self.current_preview = None;
        }
    }

    pub fn set_card_image(&self, idx: usize, image: &AnyObject) {
        if idx < self.cards.len() {
            unsafe {
                let () = msg_send![&*self.cards[idx].image_view, setImage: image];
            }
        }
    }

    pub fn render_hover_frame(
        &mut self,
        viz: &Visualizer,
        preset_idx: usize,
    ) -> Option<Retained<AnyObject>> {
        let saved_preset = viz.selected_preset_index();
        viz.select_preset(preset_idx as u32);

        for _ in 0..FRAMES_PER_TICK {
            let pcm = generate_simulated_audio(&mut self.sim_time);
            viz.add_pcm_float_stereo(&pcm);
            viz.render_frame();
        }

        let image = capture_gl_image();
        viz.select_preset(saved_preset);
        image
    }

    pub fn get_search_text(&self) -> String {
        unsafe {
            let s: Retained<NSString> = msg_send![&*self.search_field, stringValue];
            s.to_string()
        }
    }

    pub fn apply_filter(&mut self) {
        let text = self.get_search_text();
        self.filter = if text.is_empty() {
            None
        } else {
            Some(text)
        };

        for card in &self.cards {
            unsafe {
                let () = msg_send![&*card.view, removeFromSuperview];
            }
        }

        let matching: Vec<usize> = (0..self.all_presets.len())
            .filter(|&i| {
                self.filter.as_ref().map_or(true, |f| {
                    self.all_presets[i].to_lowercase().contains(&f.to_lowercase())
                })
            })
            .collect();

        let rows = (matching.len() + COLS - 1) / COLS;
        let doc_h = PAD + rows as f64 * (CARD_H + PAD) + PAD;
        let doc_w = COLS as f64 * (CARD_W + PAD) + PAD;
        unsafe {
            let () = msg_send![&*self.document_view, setFrame: CGRect::new(
                CGPoint::new(0.0, 0.0),
                CGSize::new(doc_w, doc_h),
            )];
        }

        for (grid_pos, &preset_idx) in matching.iter().enumerate() {
            let row = grid_pos / COLS;
            let col = grid_pos % COLS;
            let x = PAD + col as f64 * (CARD_W + PAD);
            let y = doc_h - PAD - row as f64 * (CARD_H + PAD) - CARD_H;
            let frame = CGRect::new(CGPoint::new(x, y), CGSize::new(CARD_W, CARD_H));
            unsafe {
                let () = msg_send![&*self.cards[preset_idx].view, setFrame: frame];
                let () = msg_send![&*self.document_view, addSubview: &*self.cards[preset_idx].view];
            }
        }

        self.visible_indices = matching;
    }

    pub fn clear_filter(&mut self) {
        unsafe {
            let () = msg_send![&*self.search_field, setStringValue: &*NSString::from_str("")];
        }
        self.apply_filter();
    }

    pub fn update_active(&mut self, index: usize) {
        let old = self.active_index;
        self.active_index = index;
        self.set_card_highlight(old, false);
        self.set_card_highlight(index, true);
    }

    fn set_card_highlight(&self, index: usize, active: bool) {
        if index >= self.cards.len() {
            return;
        }
        let card = &self.cards[index];
        unsafe {
            let layer: *mut AnyObject = msg_send![&*card.view, layer];
            if active {
                let border: *mut AnyObject = msg_send![class!(NSColor), colorWithCalibratedRed: 0.2, green: 0.55, blue: 1.0, alpha: 1.0];
                let cg: *mut AnyObject = msg_send![&*border, CGColor];
                let () = msg_send![layer, setBorderColor: cg];
                let () = msg_send![layer, setBorderWidth: 3.0];
            } else {
                let () = msg_send![layer, setBorderWidth: 0.0];
            }
        }
    }

    #[allow(dead_code)]
    pub fn update_favorites(&mut self, favorites: &HashSet<String>) {
        self.favorites = favorites.clone();
        for (i, name) in self.all_presets.iter().enumerate() {
            let is_fav = favorites.contains(name);
            unsafe {
                let star = if is_fav { "\u{2605}" } else { "\u{2606}" };
                let () = msg_send![&*self.cards[i].fav_button, setTitle: &*NSString::from_str(star)];
            }
        }
    }

    pub fn toggle_favorite(&mut self, preset_index: usize) {
        if preset_index >= self.all_presets.len() {
            return;
        }
        let name = &self.all_presets[preset_index];
        if self.favorites.contains(name) {
            self.favorites.remove(name);
        } else {
            self.favorites.insert(name.clone());
        }
        let is_fav = self.favorites.contains(name);
        unsafe {
            let star = if is_fav { "\u{2605}" } else { "\u{2606}" };
            let () = msg_send![&*self.cards[preset_index].fav_button, setTitle: &*NSString::from_str(star)];
        }
    }
}
