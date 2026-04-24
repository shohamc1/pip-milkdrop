use std::sync::atomic::{AtomicI32, Ordering};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, Sel};
use objc2::{class, define_class, msg_send, sel, AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSControlStateValueOff, NSControlStateValueOn, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::NSString;

use crate::config::{Config, Sensitivity, ShuffleMode};

pub static PENDING_ACTION: AtomicI32 = AtomicI32::new(0);

const TAG_NEXT: isize = 1;
const TAG_PREV: isize = 2;
const TAG_SEARCH: isize = 3;
const TAG_SHOW_ALL: isize = 4;
const TAG_TOGGLE_FAV: isize = 5;
const TAG_SHUFFLE_OFF: isize = 6;
const TAG_SHUFFLE_ALL: isize = 7;
const TAG_SHUFFLE_FAV: isize = 8;
const TAG_SENS_LOW: isize = 10;
const TAG_SENS_MED: isize = 11;
const TAG_SENS_HIGH: isize = 12;
const TAG_DELAY_1: isize = 20;
const TAG_DELAY_2: isize = 21;
const TAG_DELAY_4: isize = 22;
const TAG_DELAY_8: isize = 23;
const TAG_DELAY_NEVER: isize = 24;
const TAG_LOGIN: isize = 30;
pub const TAG_BROWSE: isize = 35;
const TAG_QUIT: isize = 99;
const TAG_PRESET_BASE: isize = 1000;
const TAG_FAV_BASE: isize = 5000;

define_class!(
    #[unsafe(super(NSObject))]
    struct MenuHandler;

    impl MenuHandler {
        #[unsafe(method(handleAction:))]
        fn handle_action(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let tag: isize = unsafe { msg_send![sender, tag] };
            PENDING_ACTION.store(tag as i32, Ordering::Relaxed);
        }
    }
);

impl MenuHandler {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

pub struct MenuBar {
    #[allow(dead_code)]
    status_item: Retained<NSStatusItem>,
    handler: Retained<MenuHandler>,
    #[allow(dead_code)]
    menu: Retained<NSMenu>,
    sensitivity_items: Vec<Retained<NSMenuItem>>,
    delay_items: Vec<Retained<NSMenuItem>>,
    login_item: Retained<NSMenuItem>,
    current_preset_item: Retained<NSMenuItem>,
    toggle_fav_item: Retained<NSMenuItem>,
    shuffle_items: Vec<Retained<NSMenuItem>>,
    presets_menu: Retained<NSMenu>,
    favorites_menu: Retained<NSMenu>,
    all_preset_names: Vec<String>,
    filter: Option<String>,
}

impl MenuBar {
    pub fn new() -> Self {
        let mtm = MainThreadMarker::new().unwrap();
        let handler = MenuHandler::new();
        let handler_ref: &AnyObject = handler.as_ref();
        let action_sel = sel!(handleAction:);

        let status_bar = NSStatusBar::systemStatusBar();
        let status_item = status_bar.statusItemWithLength(24.0);

        if let Some(button) = status_item.button(mtm) {
            button.setTitle(&NSString::from_str("\u{266C}"));
        }

        let menu = NSMenu::new(mtm);

        let current_preset_item = make_item("Preset: (loading)", 0, None, handler_ref, false, mtm);
        menu.addItem(&current_preset_item);

        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&make_item("Next Preset", TAG_NEXT, Some(action_sel), handler_ref, true, mtm));
        menu.addItem(&make_item("Previous Preset", TAG_PREV, Some(action_sel), handler_ref, true, mtm));

        menu.addItem(&NSMenuItem::separatorItem(mtm));

        let toggle_fav_item = make_item("\u{2665} Favorite", TAG_TOGGLE_FAV, Some(action_sel), handler_ref, true, mtm);
        menu.addItem(&toggle_fav_item);

        menu.addItem(&NSMenuItem::separatorItem(mtm));

        let presets_header = make_item("Presets", 0, None, handler_ref, true, mtm);
        let presets_menu = NSMenu::new(mtm);
        presets_menu.addItem(&make_item("Search Presets...", TAG_SEARCH, Some(action_sel), handler_ref, true, mtm));
        presets_menu.addItem(&make_item("Show All", TAG_SHOW_ALL, Some(action_sel), handler_ref, true, mtm));
        presets_menu.addItem(&NSMenuItem::separatorItem(mtm));
        presets_header.setSubmenu(Some(&presets_menu));
        menu.addItem(&presets_header);

        let fav_header = make_item("Favorites", 0, None, handler_ref, true, mtm);
        let favorites_menu = NSMenu::new(mtm);
        favorites_menu.addItem(&make_item("(none)", 0, None, handler_ref, false, mtm));
        fav_header.setSubmenu(Some(&favorites_menu));
        menu.addItem(&fav_header);

        let shuffle_menu = NSMenu::new(mtm);
        let sh_off = make_item("Off", TAG_SHUFFLE_OFF, Some(action_sel), handler_ref, true, mtm);
        let sh_all = make_item("All Presets", TAG_SHUFFLE_ALL, Some(action_sel), handler_ref, true, mtm);
        let sh_fav = make_item("Favorites Only", TAG_SHUFFLE_FAV, Some(action_sel), handler_ref, true, mtm);
        shuffle_menu.addItem(&sh_off);
        shuffle_menu.addItem(&sh_all);
        shuffle_menu.addItem(&sh_fav);
        let shuffle_header = make_item("Shuffle", 0, None, handler_ref, true, mtm);
        shuffle_header.setSubmenu(Some(&shuffle_menu));
        menu.addItem(&shuffle_header);
        let shuffle_items = vec![sh_off, sh_all, sh_fav];

        menu.addItem(&NSMenuItem::separatorItem(mtm));

        let sens_menu = NSMenu::new(mtm);
        let low = make_item("Low", TAG_SENS_LOW, Some(action_sel), handler_ref, true, mtm);
        let med = make_item("Medium", TAG_SENS_MED, Some(action_sel), handler_ref, true, mtm);
        let high = make_item("High", TAG_SENS_HIGH, Some(action_sel), handler_ref, true, mtm);
        sens_menu.addItem(&low);
        sens_menu.addItem(&med);
        sens_menu.addItem(&high);
        let sens_header = make_item("Sensitivity", 0, None, handler_ref, true, mtm);
        sens_header.setSubmenu(Some(&sens_menu));
        menu.addItem(&sens_header);
        let sensitivity_items = vec![low, med, high];

        let delay_menu = NSMenu::new(mtm);
        let d1 = make_item("1 second", TAG_DELAY_1, Some(action_sel), handler_ref, true, mtm);
        let d2 = make_item("2 seconds", TAG_DELAY_2, Some(action_sel), handler_ref, true, mtm);
        let d4 = make_item("4 seconds", TAG_DELAY_4, Some(action_sel), handler_ref, true, mtm);
        let d8 = make_item("8 seconds", TAG_DELAY_8, Some(action_sel), handler_ref, true, mtm);
        let dn = make_item("Never", TAG_DELAY_NEVER, Some(action_sel), handler_ref, true, mtm);
        delay_menu.addItem(&d1);
        delay_menu.addItem(&d2);
        delay_menu.addItem(&d4);
        delay_menu.addItem(&d8);
        delay_menu.addItem(&dn);
        let delay_header = make_item("Hide Delay", 0, None, handler_ref, true, mtm);
        delay_header.setSubmenu(Some(&delay_menu));
        menu.addItem(&delay_header);
        let delay_items = vec![d1, d2, d4, d8, dn];

        menu.addItem(&NSMenuItem::separatorItem(mtm));

        let login_item = make_item("Start at Login", TAG_LOGIN, Some(action_sel), handler_ref, true, mtm);
        menu.addItem(&login_item);

        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&make_item("Browse Presets...", TAG_BROWSE, Some(action_sel), handler_ref, true, mtm));

        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&make_item("Quit pip-milkdrop", TAG_QUIT, Some(action_sel), handler_ref, true, mtm));

        status_item.setMenu(Some(&menu));

        Self {
            status_item,
            handler,
            menu,
            sensitivity_items,
            delay_items,
            login_item,
            current_preset_item,
            toggle_fav_item,
            shuffle_items,
            presets_menu,
            favorites_menu,
            all_preset_names: Vec::new(),
            filter: None,
        }
    }

    pub fn populate_presets(&mut self, viz: &crate::visualizer::Visualizer) {
        let count = viz.playlist_size();
        self.all_preset_names = (0..count).map(|i| viz.preset_name(i)).collect();
        self.rebuild_presets_menu();
    }

    fn rebuild_presets_menu(&mut self) {
        let mtm = MainThreadMarker::new().unwrap();
        let handler_ref: &AnyObject = self.handler.as_ref();
        let action_sel = sel!(handleAction:);

        while self.presets_menu.numberOfItems() > 3 {
            self.presets_menu.removeItemAtIndex(3);
        }

        for (i, name) in self.all_preset_names.iter().enumerate() {
            if let Some(ref filter) = self.filter {
                if !name.to_lowercase().contains(&filter.to_lowercase()) {
                    continue;
                }
            }
            let tag = TAG_PRESET_BASE + i as isize;
            let item = make_item(name, tag, Some(action_sel), handler_ref, true, mtm);
            self.presets_menu.addItem(&item);
        }
    }

    pub fn rebuild_favorites(&mut self, config: &Config) {
        let mtm = MainThreadMarker::new().unwrap();
        let handler_ref: &AnyObject = self.handler.as_ref();
        let action_sel = sel!(handleAction:);

        self.favorites_menu.removeAllItems();

        if config.favorites.is_empty() {
            let item = make_item("(none)", 0, None, handler_ref, false, mtm);
            self.favorites_menu.addItem(&item);
            return;
        }

        for (i, name) in self.all_preset_names.iter().enumerate() {
            if config.favorites.contains(name) {
                let tag = TAG_FAV_BASE + i as isize;
                let item = make_item(name, tag, Some(action_sel), handler_ref, true, mtm);
                self.favorites_menu.addItem(&item);
            }
        }
    }

    fn open_search_dialog(&mut self) {
        unsafe {
            let alert: *mut AnyObject = msg_send![class!(NSAlert), alloc];
            let alert: *mut AnyObject = msg_send![alert, init];
            let alert = Retained::from_raw(alert).expect("NSAlert alloc failed");

            let () = msg_send![&*alert, setMessageText: &*NSString::from_str("Search Presets")];
            let () = msg_send![&*alert, setInformativeText: &*NSString::from_str("Enter text to filter presets:")];
            let () = msg_send![&*alert, addButtonWithTitle: &*NSString::from_str("Search")];
            let () = msg_send![&*alert, addButtonWithTitle: &*NSString::from_str("Cancel")];

            let frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(300.0, 24.0));
            let tf_alloc: *mut AnyObject = msg_send![class!(NSTextField), alloc];
            let text_field: *mut AnyObject = msg_send![tf_alloc, initWithFrame: frame];
            let text_field = Retained::from_raw(text_field).expect("NSTextField alloc failed");
            let () = msg_send![&*alert, setAccessoryView: &*text_field];

            let window: Option<Retained<AnyObject>> = msg_send![&*alert, window];
            if let Some(window) = window {
                let () = msg_send![&*window, makeFirstResponder: &*text_field];
            }

            let response: i64 = msg_send![&*alert, runModal];

            if response == 1000 {
                let string: Retained<NSString> = msg_send![&*text_field, stringValue];
                let text = string.to_string();
                self.filter = if text.is_empty() {
                    None
                } else {
                    Some(text)
                };
            }
        }

        self.rebuild_presets_menu();
    }

    pub fn update_state(&mut self, config: &Config, preset_name: &str) {
        self.current_preset_item.setTitle(&NSString::from_str(&format!("Preset: {preset_name}")));

        self.toggle_fav_item.setState(if config.favorites.contains(preset_name) {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });

        let sens_idx = match config.sensitivity {
            Sensitivity::Low => 0,
            Sensitivity::Medium => 1,
            Sensitivity::High => 2,
        };
        for (i, item) in self.sensitivity_items.iter().enumerate() {
            item.setState(if i == sens_idx {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }

        let delay_vals: [u64; 5] = [1, 2, 4, 8, 0];
        for (i, item) in self.delay_items.iter().enumerate() {
            item.setState(if delay_vals[i] == config.hide_delay_secs {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }

        let shuffle_idx = match config.shuffle_mode {
            ShuffleMode::Off => 0,
            ShuffleMode::All => 1,
            ShuffleMode::Favorites => 2,
        };
        for (i, item) in self.shuffle_items.iter().enumerate() {
            item.setState(if i == shuffle_idx {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }

        self.login_item.setState(if config.start_at_login {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
    }

    pub fn handle_pending_action(
        &mut self,
        config: &mut Config,
        viz: &crate::visualizer::Visualizer,
    ) -> i32 {
        let tag = PENDING_ACTION.swap(0, Ordering::Relaxed);
        if tag == 0 {
            return 0;
        }

        let fav_indices = || -> Vec<u32> {
            self.all_preset_names
                .iter()
                .enumerate()
                .filter(|(_, name)| config.favorites.contains(*name))
                .map(|(i, _)| i as u32)
                .collect()
        };

        match tag {
            t if t == TAG_NEXT as i32 => match config.shuffle_mode {
                ShuffleMode::Off => viz.select_next(),
                ShuffleMode::All => {
                    let count = self.all_preset_names.len() as u32;
                    viz.select_random_from(&vec_count(count));
                }
                ShuffleMode::Favorites => viz.select_random_from(&fav_indices()),
            },
            t if t == TAG_PREV as i32 => match config.shuffle_mode {
                ShuffleMode::Off => viz.select_previous(),
                ShuffleMode::All => {
                    let count = self.all_preset_names.len() as u32;
                    viz.select_random_from(&vec_count(count));
                }
                ShuffleMode::Favorites => viz.select_random_from(&fav_indices()),
            },
            t if t == TAG_SEARCH as i32 => {
                self.open_search_dialog();
            }
            t if t == TAG_SHOW_ALL as i32 => {
                self.filter = None;
                self.rebuild_presets_menu();
            }
            t if t == TAG_TOGGLE_FAV as i32 => {
                let name = viz.preset_name(viz.selected_preset_index());
                if config.favorites.contains(&name) {
                    config.favorites.remove(&name);
                } else {
                    config.favorites.insert(name);
                }
                config.save();
                self.rebuild_favorites(config);
            }
            t if t == TAG_SHUFFLE_OFF as i32 => {
                config.shuffle_mode = ShuffleMode::Off;
                config.save();
            }
            t if t == TAG_SHUFFLE_ALL as i32 => {
                config.shuffle_mode = ShuffleMode::All;
                config.save();
            }
            t if t == TAG_SHUFFLE_FAV as i32 => {
                config.shuffle_mode = ShuffleMode::Favorites;
                config.save();
            }
            t if t == TAG_SENS_LOW as i32 => {
                config.sensitivity = Sensitivity::Low;
                config.save();
            }
            t if t == TAG_SENS_MED as i32 => {
                config.sensitivity = Sensitivity::Medium;
                config.save();
            }
            t if t == TAG_SENS_HIGH as i32 => {
                config.sensitivity = Sensitivity::High;
                config.save();
            }
            t if t == TAG_DELAY_1 as i32 => {
                config.hide_delay_secs = 1;
                config.save();
            }
            t if t == TAG_DELAY_2 as i32 => {
                config.hide_delay_secs = 2;
                config.save();
            }
            t if t == TAG_DELAY_4 as i32 => {
                config.hide_delay_secs = 4;
                config.save();
            }
            t if t == TAG_DELAY_8 as i32 => {
                config.hide_delay_secs = 8;
                config.save();
            }
            t if t == TAG_DELAY_NEVER as i32 => {
                config.hide_delay_secs = 0;
                config.save();
            }
            t if t == TAG_LOGIN as i32 => {
                config.start_at_login = !config.start_at_login;
                crate::config::update_launch_agent(config.start_at_login);
                config.save();
            }
            t if t == TAG_QUIT as i32 => return -1,
            t if t >= TAG_PRESET_BASE as i32 && t < TAG_FAV_BASE as i32 => {
                let index = (t - TAG_PRESET_BASE as i32) as u32;
                viz.select_preset(index);
            }
            t if t >= TAG_FAV_BASE as i32 => {
                let index = (t - TAG_FAV_BASE as i32) as u32;
                viz.select_preset(index);
            }
            _ => {}
        }

        tag
    }
}

fn vec_count(count: u32) -> Vec<u32> {
    (0..count).collect()
}

fn make_item(
    title: &str,
    tag: isize,
    action: Option<Sel>,
    target: &AnyObject,
    enabled: bool,
    mtm: MainThreadMarker,
) -> Retained<NSMenuItem> {
    unsafe {
        let item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(""),
        );
        if tag != 0 {
            item.setTag(tag);
        }
        if action.is_some() {
            item.setTarget(Some(target));
        }
        item.setEnabled(enabled);
        item
    }
}
