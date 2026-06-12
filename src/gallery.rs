use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, Ordering};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{
    class, define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSView, NSWindow,
    NSWindowStyleMask,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGColor;
use objc2_foundation::NSString;

use crate::visualizer::Visualizer;

pub static GALLERY_ACTION: AtomicI32 = AtomicI32::new(0);
pub static GALLERY_HOVER: AtomicI32 = AtomicI32::new(-1);

pub const GA_SEARCH: i32 = 1;
pub const GA_CLEAR: i32 = 2;
pub const GA_TAB_ALL: i32 = 3;
pub const GA_TAB_FAV: i32 = 4;
pub const GA_SECTION_BASE: i32 = 100;
pub const GA_SELECT_BASE: i32 = 1000;
pub const GA_FAV_BASE: i32 = 5000;

const PREVIEW_W: usize = 300;
const PREVIEW_H: usize = 300;
const CARD_W: f64 = 148.0;
const CARD_H: f64 = 188.0;
const IMG_SIZE: f64 = 132.0;
const PAD: f64 = 6.0;
const MIN_COLS: usize = 2;
const WARMUP_INITIAL: usize = 8;
const FRAMES_PER_TICK: usize = 2;
const SECTION_HEADER_H: f64 = 24.0;
const HEADER_PAD: f64 = 10.0;
const SEARCH_H: f64 = 28.0;
const TAB_H: f64 = 28.0;
const HEADER_GAP: f64 = 6.0;

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

        #[unsafe(method(tabAllClicked:))]
        fn tab_all_clicked(&self, _sender: Option<&AnyObject>) {
            GALLERY_ACTION.store(GA_TAB_ALL, Ordering::Relaxed);
        }

        #[unsafe(method(tabFavClicked:))]
        fn tab_fav_clicked(&self, _sender: Option<&AnyObject>) {
            GALLERY_ACTION.store(GA_TAB_FAV, Ordering::Relaxed);
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

define_class!(
    #[unsafe(super(NSView))]
    #[ivars = (usize,)]
    struct SectionHeaderView;

    impl SectionHeaderView {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &objc2_app_kit::NSEvent) {
            let section_idx = self.ivars().0;
            GALLERY_ACTION.store(GA_SECTION_BASE + section_idx as i32, Ordering::Relaxed);
        }

        #[unsafe(method(resetCursorRects))]
        fn reset_cursor_rects(&self) {
            unsafe {
                let cursor: *mut AnyObject = msg_send![class!(NSCursor), pointingHandCursor];
                let bounds: CGRect = msg_send![self, bounds];
                let () = msg_send![self, addCursorRect: bounds cursor: cursor];
            }
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

fn thumbnail_path(dir: &PathBuf, name: &str) -> PathBuf {
    let safe = name.replace('/', "_").replace('\\', "_").replace(':', "_");
    dir.join(format!("{safe}.rgba"))
}

fn save_thumbnail(dir: &PathBuf, name: &str, image: &AnyObject) {
    let path = thumbnail_path(dir, name);
    unsafe {
        let size: CGSize = msg_send![image, size];
        let w = size.width as usize;
        let h = size.height as usize;
        if w == 0 || h == 0 {
            return;
        }
        let reps: *mut AnyObject = msg_send![image, representations];
        let count: usize = msg_send![reps, count];
        for i in 0..count {
            let rep: *mut AnyObject = msg_send![reps, objectAtIndex: i];
            let data_ptr: *mut u8 = msg_send![rep, bitmapData];
            if data_ptr.is_null() {
                continue;
            }
            let pixels = std::slice::from_raw_parts(data_ptr, w * h * 4);
            let mut out = Vec::with_capacity(8 + w * h * 4);
            out.extend_from_slice(&(w as u32).to_le_bytes());
            out.extend_from_slice(&(h as u32).to_le_bytes());
            out.extend_from_slice(pixels);
            let _ = std::fs::write(&path, out);
            return;
        }
    }
}

fn load_thumbnail(dir: &PathBuf, name: &str) -> Option<Retained<AnyObject>> {
    let path = thumbnail_path(dir, name);
    let data = std::fs::read(&path).ok()?;
    if data.len() < 8 {
        return None;
    }
    let w = u32::from_le_bytes(data[0..4].try_into().ok()?) as usize;
    let h = u32::from_le_bytes(data[4..8].try_into().ok()?) as usize;
    if data.len() != 8 + w * h * 4 {
        return None;
    }
    unsafe { create_nsimage_from_pixels(&data[8..], w, h) }
}

struct Card {
    view: Retained<CardView>,
    image_view: Retained<AnyObject>,
    #[allow(dead_code)]
    name_label: Retained<AnyObject>,
    fav_button: Retained<AnyObject>,
}

struct SectionInfo {
    id: usize,
    label: &'static str,
    start: usize,
    count: usize,
}

pub struct Gallery {
    window: Retained<NSWindow>,
    #[allow(dead_code)]
    scroll_view: Retained<AnyObject>,
    content_view: Retained<NSView>,
    document_view: Retained<AnyObject>,
    search_field: Retained<AnyObject>,
    search_btn: Retained<AnyObject>,
    clear_btn: Retained<AnyObject>,
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
    stock_count: usize,
    section_views: Vec<Retained<SectionHeaderView>>,
    section_labels: Vec<Retained<AnyObject>>,
    collapsed_sections: HashSet<usize>,
    show_favorites_only: bool,
    tab_all_btn: Retained<AnyObject>,
    tab_fav_btn: Retained<AnyObject>,
    thumbnail_dir: PathBuf,

    sim_time: f64,
    current_preview: Option<usize>,
    preview_frames: usize,
    saved_preset: u32,
    initial_queue: Vec<usize>,
    initial_queued: HashSet<usize>,
    last_layout_w: f64,
    last_layout_h: f64,
}

impl Gallery {
    fn sections(&self) -> Vec<SectionInfo> {
        let mut sections = Vec::new();
        if self.stock_count > 0 {
            sections.push(SectionInfo {
                id: 0,
                label: "Stock Presets",
                start: 0,
                count: self.stock_count,
            });
        }
        let user_count = self.all_presets.len().saturating_sub(self.stock_count);
        if user_count > 0 {
            sections.push(SectionInfo {
                id: 1,
                label: "My Presets",
                start: self.stock_count,
                count: user_count,
            });
        }
        sections
    }

    fn section_header_title(label: &str, collapsed: bool) -> String {
        let arrow = if collapsed { "\u{25B6}" } else { "\u{25BE}" };
        format!("{arrow} {label}")
    }

    pub fn new(
        presets: &[String],
        stock_count: usize,
        favorites: &HashSet<String>,
        active_index: usize,
        mtm: MainThreadMarker,
    ) -> Self {
        let handler = GalleryHandler::new();
        let handler_ref: &AnyObject = handler.as_ref();
        let fav_sel = sel!(favClicked:);
        let search_sel = sel!(searchClicked:);
        let clear_sel = sel!(clearClicked:);
        let tab_all_sel = sel!(tabAllClicked:);
        let tab_fav_sel = sel!(tabFavClicked:);

        let thumbnail_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("pip-milkdrop")
            .join("thumbnails");
        let _ = std::fs::create_dir_all(&thumbnail_dir);

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

        let search_y = win_h - HEADER_PAD - SEARCH_H;
        let search_field: Retained<AnyObject> = unsafe {
            let tf: *mut AnyObject = msg_send![class!(NSSearchField), alloc];
            let tf: *mut AnyObject = msg_send![tf, initWithFrame: CGRect::new(
                CGPoint::new(PAD, search_y),
                CGSize::new(win_w - 2.0 * HEADER_PAD - 150.0, SEARCH_H),
            )];
            let tf = Retained::from_raw(tf).unwrap();
            let () = msg_send![&*tf, setEditable: true];
            let () =
                msg_send![&*tf, setPlaceholderString: &*NSString::from_str("Filter presets...")];
            let () = msg_send![&*tf, setTarget: handler_ref];
            let () = msg_send![&*tf, setAction: search_sel];
            // Make Return in the search field apply the filter; the explicit button remains
            // for discoverability.
            let () = msg_send![&*tf, setAutoresizingMask: 0usize];
            tf
        };
        unsafe {
            let () = msg_send![&*content_view, addSubview: &*search_field];
        }

        let search_btn: Retained<AnyObject> = unsafe {
            let btn: *mut AnyObject = msg_send![class!(NSButton), alloc];
            let btn: *mut AnyObject = msg_send![btn, initWithFrame: CGRect::new(
                CGPoint::new(win_w - HEADER_PAD - 144.0, search_y),
                CGSize::new(70.0, SEARCH_H),
            )];
            let btn = Retained::from_raw(btn).unwrap();
            let () = msg_send![&*btn, setTitle: &*NSString::from_str("Search")];
            let () = msg_send![&*btn, setTarget: handler_ref];
            let () = msg_send![&*btn, setAction: search_sel];
            let () = msg_send![&*btn, setBezelStyle: 1isize];
            let () = msg_send![&*btn, setAutoresizingMask: 0usize];
            btn
        };
        unsafe {
            let () = msg_send![&*content_view, addSubview: &*search_btn];
        }

        let clear_btn: Retained<AnyObject> = unsafe {
            let btn: *mut AnyObject = msg_send![class!(NSButton), alloc];
            let btn: *mut AnyObject = msg_send![btn, initWithFrame: CGRect::new(
                CGPoint::new(win_w - HEADER_PAD - 68.0, search_y),
                CGSize::new(70.0, SEARCH_H),
            )];
            let btn = Retained::from_raw(btn).unwrap();
            let () = msg_send![&*btn, setTitle: &*NSString::from_str("Clear")];
            let () = msg_send![&*btn, setTarget: handler_ref];
            let () = msg_send![&*btn, setAction: clear_sel];
            let () = msg_send![&*btn, setBezelStyle: 1isize];
            let () = msg_send![&*btn, setAutoresizingMask: 0usize];
            btn
        };
        unsafe {
            let () = msg_send![&*content_view, addSubview: &*clear_btn];
        }

        let tab_y = search_y - TAB_H - 4.0;
        let tab_all_btn: Retained<AnyObject> = unsafe {
            let btn: *mut AnyObject = msg_send![class!(NSButton), alloc];
            let btn: *mut AnyObject = msg_send![btn, initWithFrame: CGRect::new(
                CGPoint::new(HEADER_PAD, tab_y),
                CGSize::new(96.0, TAB_H),
            )];
            let btn = Retained::from_raw(btn).unwrap();
            let () = msg_send![&*btn, setTitle: &*NSString::from_str("All")];
            let () = msg_send![&*btn, setTarget: handler_ref];
            let () = msg_send![&*btn, setAction: tab_all_sel];
            let () = msg_send![&*btn, setBezelStyle: 1isize];
            let () = msg_send![&*btn, setAutoresizingMask: 0usize];
            btn
        };
        unsafe {
            let () = msg_send![&*content_view, addSubview: &*tab_all_btn];
        }

        let tab_fav_btn: Retained<AnyObject> = unsafe {
            let btn: *mut AnyObject = msg_send![class!(NSButton), alloc];
            let btn: *mut AnyObject = msg_send![btn, initWithFrame: CGRect::new(
                CGPoint::new(HEADER_PAD + 100.0, tab_y),
                CGSize::new(116.0, TAB_H),
            )];
            let btn = Retained::from_raw(btn).unwrap();
            let () = msg_send![&*btn, setTitle: &*NSString::from_str("Favorites")];
            let () = msg_send![&*btn, setTarget: handler_ref];
            let () = msg_send![&*btn, setAction: tab_fav_sel];
            let () = msg_send![&*btn, setBezelStyle: 1isize];
            let () = msg_send![&*btn, setAutoresizingMask: 0usize];
            btn
        };
        unsafe {
            let () = msg_send![&*content_view, addSubview: &*tab_fav_btn];
        }

        let grid_h = (tab_y - HEADER_GAP).max(120.0);
        let scroll_rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(win_w, grid_h));
        let scroll_view: Retained<AnyObject> = unsafe {
            let sv: *mut AnyObject = msg_send![class!(NSScrollView), alloc];
            let sv: *mut AnyObject = msg_send![sv, initWithFrame: scroll_rect];
            let sv = Retained::from_raw(sv).unwrap();
            let () = msg_send![&*sv, setHasVerticalScroller: true];
            let () = msg_send![&*sv, setAutohidesScrollers: false];
            let () = msg_send![&*sv, setScrollerStyle: 0isize]; // NSScrollerStyleLegacy: always visible, avoids hidden overlay affordance.
            let () = msg_send![&*sv, setAutoresizingMask: 0usize];
            sv
        };
        unsafe {
            let () = msg_send![&*content_view, addSubview: &*scroll_view];
        }

        let total = presets.len();
        let small_font: Retained<AnyObject> =
            unsafe { msg_send![class!(NSFont), systemFontOfSize: 11.0] };
        let star_font: Retained<AnyObject> =
            unsafe { msg_send![class!(NSFont), systemFontOfSize: 16.0] };
        let white_color: *mut AnyObject = unsafe { msg_send![class!(NSColor), whiteColor] };

        let mut cards = Vec::with_capacity(total);
        for (i, name) in presets.iter().enumerate() {
            let card_rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(CARD_W, CARD_H));
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
                let cg: *mut CGColor = msg_send![&*bg, CGColor];
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
                CGPoint::new((CARD_W - IMG_SIZE) / 2.0, 34.0),
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

            let label_rect = CGRect::new(CGPoint::new(6.0, 4.0), CGSize::new(CARD_W - 12.0, 24.0));
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
                let () = msg_send![&*tf, setUsesSingleLineMode: false];
                let () = msg_send![&*tf, setLineBreakMode: 4isize]; // NSLineBreakByTruncatingTail
                let clean = name.strip_suffix(".milk").unwrap_or(name);
                let display_name = clean.to_string();
                let () = msg_send![&*tf, setStringValue: &*NSString::from_str(&display_name)];
                let () = msg_send![&*tf, setToolTip: &*NSString::from_str(&display_name)];
                tf
            };
            unsafe {
                let () = msg_send![&*card_view, addSubview: &*name_label];
            }

            let fav_rect = CGRect::new(
                CGPoint::new(CARD_W - 28.0, IMG_SIZE + 34.0 - 26.0),
                CGSize::new(24.0, 24.0),
            );
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

        let doc_w = 5.0 * (CARD_W + PAD) + PAD;
        let doc_h = 1.0f64;
        let doc_rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(doc_w, doc_h));
        let document_view: Retained<AnyObject> = unsafe {
            let dv: *mut AnyObject = msg_send![class!(NSView), alloc];
            let dv: *mut AnyObject = msg_send![dv, initWithFrame: doc_rect];
            Retained::from_raw(dv).unwrap()
        };
        unsafe {
            let () = msg_send![&*scroll_view, setDocumentView: &*document_view];
        }

        let mut preview_images: Vec<Option<Retained<AnyObject>>> = vec![None; total];
        let mut cached_set = HashSet::new();
        for (i, name) in presets.iter().enumerate() {
            if let Some(img) = load_thumbnail(&thumbnail_dir, name) {
                if i < cards.len() {
                    unsafe {
                        let () = msg_send![&*cards[i].image_view, setImage: &*img];
                    }
                }
                preview_images[i] = Some(img);
                cached_set.insert(i);
            }
        }
        eprintln!(
            "[pip-milkdrop] Loaded {}/{} cached thumbnails",
            cached_set.len(),
            total
        );

        let collapsed_sections = HashSet::new();

        let visible_indices: Vec<usize> = (0..total).collect();

        let mut gallery = Self {
            window,
            scroll_view,
            content_view,
            document_view,
            search_field,
            search_btn,
            clear_btn,
            handler,
            cards,
            preview_images,
            all_presets: presets.to_vec(),
            visible_indices,
            active_index,
            favorites: favorites.clone(),
            filter: None,
            is_open: false,
            stock_count,
            section_views: Vec::new(),
            section_labels: Vec::new(),
            collapsed_sections,
            show_favorites_only: false,
            tab_all_btn,
            tab_fav_btn,
            thumbnail_dir,
            sim_time: 0.0,
            current_preview: None,
            preview_frames: 0,
            saved_preset: 0,
            initial_queue: Vec::new(),
            initial_queued: HashSet::new(),
            last_layout_w: 0.0,
            last_layout_h: 0.0,
        };

        gallery.layout_chrome();
        gallery.relayout();

        let mut initial_queue = Vec::new();
        let mut initial_queued = HashSet::new();
        for idx in 0..total {
            if !cached_set.contains(&idx) {
                initial_queued.insert(idx);
                initial_queue.push(idx);
            }
        }
        gallery.initial_queue = initial_queue;
        gallery.initial_queued = initial_queued;

        gallery.update_active(active_index);
        gallery.update_tab_style();
        gallery
    }

    fn layout_chrome(&self) {
        let bounds: CGRect = unsafe { msg_send![&*self.content_view, bounds] };
        let w = bounds.size.width.max(400.0);
        let h = bounds.size.height.max(300.0);

        let button_gap = 6.0;
        let search_button_w = 70.0;
        let clear_button_w = 62.0;
        let search_y = h - HEADER_PAD - SEARCH_H;
        let clear_x = w - HEADER_PAD - clear_button_w;
        let search_btn_x = clear_x - button_gap - search_button_w;
        let search_w = (search_btn_x - HEADER_PAD - button_gap).max(120.0);

        let tab_y = search_y - HEADER_GAP - TAB_H;
        let scroll_h = (tab_y - HEADER_GAP).max(80.0);

        unsafe {
            let () = msg_send![&*self.search_field, setFrame: CGRect::new(
                CGPoint::new(HEADER_PAD, search_y),
                CGSize::new(search_w, SEARCH_H),
            )];
            let () = msg_send![&*self.search_btn, setFrame: CGRect::new(
                CGPoint::new(search_btn_x, search_y),
                CGSize::new(search_button_w, SEARCH_H),
            )];
            let () = msg_send![&*self.clear_btn, setFrame: CGRect::new(
                CGPoint::new(clear_x, search_y),
                CGSize::new(clear_button_w, SEARCH_H),
            )];
            let () = msg_send![&*self.tab_all_btn, setFrame: CGRect::new(
                CGPoint::new(HEADER_PAD, tab_y),
                CGSize::new(96.0, TAB_H),
            )];
            let () = msg_send![&*self.tab_fav_btn, setFrame: CGRect::new(
                CGPoint::new(HEADER_PAD + 100.0, tab_y),
                CGSize::new(116.0, TAB_H),
            )];
            let () = msg_send![&*self.scroll_view, setFrame: CGRect::new(
                CGPoint::new(0.0, 0.0),
                CGSize::new(w, scroll_h),
            )];
        }
    }

    fn relayout(&mut self) {
        for card in &self.cards {
            unsafe {
                let () = msg_send![&*card.view, removeFromSuperview];
            }
        }
        for sv in &self.section_views {
            unsafe {
                let () = msg_send![&*sv, removeFromSuperview];
            }
        }
        for sl in &self.section_labels {
            unsafe {
                let () = msg_send![&*sl, removeFromSuperview];
            }
        }
        self.section_views.clear();
        self.section_labels.clear();

        let mtm = MainThreadMarker::new().unwrap();
        let header_font: Retained<AnyObject> =
            unsafe { msg_send![class!(NSFont), boldSystemFontOfSize: 14.0] };
        let white_color: *mut AnyObject = unsafe { msg_send![class!(NSColor), whiteColor] };

        let viewport_bounds: CGRect = unsafe { msg_send![&*self.scroll_view, frame] };
        let viewport_w = viewport_bounds.size.width.max(CARD_W + PAD * 2.0);
        let cols = ((viewport_w - PAD) / (CARD_W + PAD))
            .floor()
            .max(MIN_COLS as f64) as usize;
        let used_w = cols as f64 * CARD_W;
        let gap = if cols > 1 {
            ((viewport_w - used_w) / (cols as f64 + 1.0)).clamp(PAD, 18.0)
        } else {
            PAD
        };
        let grid_w = cols as f64 * CARD_W + (cols as f64 + 1.0) * gap;
        let doc_w = viewport_w.max(grid_w);

        let sections = self.sections();
        let filter_lc = self.filter.as_ref().map(|f| f.to_lowercase());
        let matching: Vec<usize> = (0..self.all_presets.len())
            .filter(|&i| {
                if self.show_favorites_only && !self.favorites.contains(&self.all_presets[i]) {
                    return false;
                }
                filter_lc
                    .as_ref()
                    .map_or(true, |f| self.all_presets[i].to_lowercase().contains(f))
            })
            .collect();

        let mut filter_sections: Vec<(usize, &str, Vec<usize>)> = Vec::new();
        for sec in &sections {
            let sec_matching: Vec<usize> = matching
                .iter()
                .copied()
                .filter(|&i| i >= sec.start && i < sec.start + sec.count)
                .collect();
            if !sec_matching.is_empty() {
                filter_sections.push((sec.id, sec.label, sec_matching));
            }
        }

        let mut doc_h = 0.0f64;
        for (section_id, _, indices) in &filter_sections {
            let collapsed = self.collapsed_sections.contains(section_id);
            doc_h += SECTION_HEADER_H;
            if !collapsed {
                let rows = (indices.len() + cols - 1) / cols;
                doc_h += rows as f64 * (CARD_H + PAD) + PAD;
            } else {
                doc_h += PAD;
            }
        }
        if filter_sections.is_empty() {
            doc_h = viewport_bounds.size.height.max(80.0);
        }

        if filter_sections.is_empty() {
            let message = if self.show_favorites_only {
                "No favorite presets yet. Star presets in All to collect them here."
            } else if self.filter.is_some() {
                "No presets match this search."
            } else {
                "No presets found."
            };
            let empty_label: Retained<AnyObject> = unsafe {
                let tf: *mut AnyObject = msg_send![class!(NSTextField), alloc];
                let tf: *mut AnyObject = msg_send![tf,
                    initWithFrame: CGRect::new(
                        CGPoint::new(PAD, (doc_h - 40.0).max(PAD)),
                        CGSize::new(doc_w - PAD * 2.0, 32.0)
                    )
                ];
                let tf = Retained::from_raw(tf).unwrap();
                let () = msg_send![&*tf, setEditable: false];
                let () = msg_send![&*tf, setSelectable: false];
                let () = msg_send![&*tf, setBezeled: false];
                let () = msg_send![&*tf, setDrawsBackground: false];
                let () = msg_send![&*tf, setAlignment: 1isize];
                let gray: *mut AnyObject = msg_send![class!(NSColor), secondaryLabelColor];
                let () = msg_send![&*tf, setTextColor: gray];
                let () = msg_send![&*tf, setStringValue: &*NSString::from_str(message)];
                tf
            };
            unsafe {
                let () = msg_send![&*self.document_view, addSubview: &*empty_label];
            }
            self.section_labels.push(empty_label);
        }

        let mut y_from_top = 0.0f64;
        let mut displayed = Vec::new();
        for (section_id, label, indices) in &filter_sections {
            let collapsed = self.collapsed_sections.contains(section_id);
            let header_y = doc_h - y_from_top - SECTION_HEADER_H;
            let header_rect = CGRect::new(
                CGPoint::new(PAD, header_y),
                CGSize::new(doc_w - PAD * 2.0, SECTION_HEADER_H),
            );

            let header_view: Retained<SectionHeaderView> = unsafe {
                let view = SectionHeaderView::alloc(mtm).set_ivars((*section_id,));
                msg_send![super(view), initWithFrame: header_rect]
            };

            let title = Self::section_header_title(label, collapsed);
            let label_view: Retained<AnyObject> = unsafe {
                let tf: *mut AnyObject = msg_send![class!(NSTextField), alloc];
                let tf: *mut AnyObject = msg_send![tf,
                    initWithFrame: CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(doc_w - PAD * 2.0, SECTION_HEADER_H))
                ];
                let tf = Retained::from_raw(tf).unwrap();
                let () = msg_send![&*tf, setEditable: false];
                let () = msg_send![&*tf, setSelectable: false];
                let () = msg_send![&*tf, setBezeled: false];
                let () = msg_send![&*tf, setDrawsBackground: false];
                let () = msg_send![&*tf, setFont: &*header_font];
                let () = msg_send![&*tf, setTextColor: white_color];
                let () = msg_send![&*tf, setStringValue: &*NSString::from_str(&title)];
                tf
            };
            unsafe {
                let () = msg_send![&*header_view, addSubview: &*label_view];
                let () = msg_send![&*self.document_view, addSubview: &*header_view];
            }
            self.section_views.push(header_view);
            self.section_labels.push(label_view);

            y_from_top += SECTION_HEADER_H;

            if !collapsed {
                for (gi, &preset_idx) in indices.iter().enumerate() {
                    let row = gi / cols;
                    let col = gi % cols;
                    let x = gap + col as f64 * (CARD_W + gap);
                    let y = doc_h - y_from_top - row as f64 * (CARD_H + PAD) - CARD_H;
                    let frame = CGRect::new(CGPoint::new(x, y), CGSize::new(CARD_W, CARD_H));
                    unsafe {
                        let () = msg_send![&*self.cards[preset_idx].view, setFrame: frame];
                        let () = msg_send![&*self.document_view, addSubview: &*self.cards[preset_idx].view];
                    }
                    displayed.push(preset_idx);
                }
                let rows = (indices.len() + cols - 1) / cols;
                y_from_top += rows as f64 * (CARD_H + PAD) + PAD;
            } else {
                y_from_top += PAD;
            }
        }

        unsafe {
            let () = msg_send![&*self.document_view, setFrame: CGRect::new(
                CGPoint::new(0.0, 0.0),
                CGSize::new(doc_w, doc_h.max(1.0)),
            )];
        }

        self.visible_indices = displayed;
        self.last_layout_w = viewport_bounds.size.width;
        self.last_layout_h = viewport_bounds.size.height;
    }

    pub fn sync_layout_to_bounds(&mut self) {
        self.layout_chrome();
        let bounds: CGRect = unsafe { msg_send![&*self.scroll_view, frame] };
        if (bounds.size.width - self.last_layout_w).abs() > 0.5
            || (bounds.size.height - self.last_layout_h).abs() > 0.5
        {
            self.relayout();
        }
    }

    fn scroll_to_top(&self) {
        unsafe {
            let clip_view: Retained<AnyObject> = msg_send![&*self.scroll_view, contentView];
            let clip_bounds: CGRect = msg_send![&*clip_view, bounds];
            let doc_frame: CGRect = msg_send![&*self.document_view, frame];
            let target_y = (doc_frame.size.height - clip_bounds.size.height).max(0.0);
            let () = msg_send![&*clip_view, scrollToPoint: CGPoint::new(0.0, target_y)];
            let () = msg_send![&*self.scroll_view, reflectScrolledClipView: &*clip_view];
        }
    }

    pub fn toggle_section(&mut self, section_idx: usize) {
        if self.collapsed_sections.contains(&section_idx) {
            self.collapsed_sections.remove(&section_idx);
        } else {
            self.collapsed_sections.insert(section_idx);
        }
        self.relayout();
        self.scroll_to_top();
    }

    fn update_tab_style(&self) {
        let accent: *mut AnyObject = unsafe { msg_send![class!(NSColor), controlColor] };
        let highlight: *mut AnyObject = unsafe { msg_send![class!(NSColor), selectedControlColor] };
        unsafe {
            let active = if self.show_favorites_only {
                &self.tab_fav_btn
            } else {
                &self.tab_all_btn
            };
            let inactive = if self.show_favorites_only {
                &self.tab_all_btn
            } else {
                &self.tab_fav_btn
            };
            let () = msg_send![&**active, setBezelColor: highlight];
            let () = msg_send![&**inactive, setBezelColor: accent];
        }
    }

    pub fn set_tab_favorites(&mut self) {
        self.show_favorites_only = true;
        self.update_tab_style();
        self.relayout();
        self.scroll_to_top();
    }

    pub fn set_tab_all(&mut self) {
        self.show_favorites_only = false;
        self.update_tab_style();
        self.relayout();
        self.scroll_to_top();
    }

    pub fn show(&mut self) {
        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);
        unsafe {
            let _: bool =
                msg_send![&app, setActivationPolicy: NSApplicationActivationPolicy::Regular];
            let () = msg_send![&app, activateIgnoringOtherApps: true];
        }
        self.window.makeKeyAndOrderFront(None);
        self.is_open = true;

        self.scroll_to_top();
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
                let _: bool =
                    msg_send![&app, setActivationPolicy: NSApplicationActivationPolicy::Accessory];
            }
        }
    }

    pub fn tick(&mut self, viz: &Visualizer) {
        if !self.is_open {
            return;
        }

        self.sync_layout_to_bounds();

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
                    if idx < self.all_presets.len() {
                        save_thumbnail(&self.thumbnail_dir, &self.all_presets[idx], &image);
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
        self.filter = if text.is_empty() { None } else { Some(text) };
        self.relayout();
        self.scroll_to_top();
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
                let cg: *mut CGColor = msg_send![&*border, CGColor];
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
                let () =
                    msg_send![&*self.cards[i].fav_button, setTitle: &*NSString::from_str(star)];
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
        if self.show_favorites_only {
            self.relayout();
        }
    }
}
