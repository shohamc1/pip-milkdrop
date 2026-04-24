use std::sync::atomic::{AtomicI32, Ordering};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, Sel};
use objc2::{define_class, msg_send, sel, AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSControlStateValueOff, NSControlStateValueOn, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
};
use objc2_foundation::NSString;

use crate::config::{Config, Sensitivity};

pub static PENDING_ACTION: AtomicI32 = AtomicI32::new(0);

const TAG_NEXT: isize = 1;
const TAG_PREV: isize = 2;
const TAG_SENS_LOW: isize = 10;
const TAG_SENS_MED: isize = 11;
const TAG_SENS_HIGH: isize = 12;
const TAG_DELAY_1: isize = 20;
const TAG_DELAY_2: isize = 21;
const TAG_DELAY_4: isize = 22;
const TAG_DELAY_8: isize = 23;
const TAG_DELAY_NEVER: isize = 24;
const TAG_LOGIN: isize = 30;
const TAG_QUIT: isize = 99;

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
    #[allow(dead_code)]
    handler: Retained<MenuHandler>,
    #[allow(dead_code)]
    menu: Retained<NSMenu>,
    sensitivity_items: Vec<Retained<NSMenuItem>>,
    delay_items: Vec<Retained<NSMenuItem>>,
    login_item: Retained<NSMenuItem>,
    current_preset_item: Retained<NSMenuItem>,
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
        }
    }

    pub fn update_state(&self, config: &Config, preset_name: &str) {
        self.current_preset_item.setTitle(&NSString::from_str(&format!("Preset: {preset_name}")));

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

        self.login_item.setState(if config.start_at_login {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
    }

    pub fn handle_pending_action(&self, config: &mut Config, viz: &crate::visualizer::Visualizer) -> bool {
        let tag = PENDING_ACTION.swap(0, Ordering::Relaxed);
        if tag == 0 {
            return false;
        }

        match tag {
            t if t == TAG_NEXT as i32 => viz.select_next(),
            t if t == TAG_PREV as i32 => viz.select_previous(),
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
            t if t == TAG_QUIT as i32 => return true,
            _ => {}
        }

        false
    }
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
