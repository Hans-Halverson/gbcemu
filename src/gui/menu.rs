use std::str::FromStr;

use eframe::egui;
use muda::{
    CheckMenuItem, Menu, MenuEvent, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
    accelerator::{Accelerator, Code, Modifiers},
};

use crate::{
    audio::NUM_AUDIO_CHANNELS,
    emulator::Command,
    gui::shell::{EmulatorShellApp, ScreenColorPalette},
    save_file::NUM_QUICK_SAVE_SLOTS,
};

// Submenu IDs
const APP_NAME_SUBMENU_ID: &str = "app_name";
const EMULATOR_SUBMENU_ID: &str = "emulator";
const QUICK_SAVE_SUBMENU_ID: &str = "quick_save";
const LOAD_QUICK_SAVE_SUBMENU_ID: &str = "load_quick_save";
const COLOR_PALETTE_SUBMENU_ID: &str = "color_palette";
const AUDIO_SUBMENU_ID: &str = "audio";
const AUDIO_DEBUG_SUBMENU_ID: &str = "audio_debug";
const DEBUG_SUBMENU_ID: &str = "debug";
const WINDOW_SUBMENU_ID: &str = "window";

// Menu item IDs
const QUIT_ITEM_ID: &str = "quit";
const PAUSE_ITEM_ID: &str = "pause";
const SAVE_ITEM_ID: &str = "save";
const QUICK_SAVE_ITEM_ID_PREFIX: &str = "quick_save_";
const LOAD_QUICK_SAVE_ITEM_ID_PREFIX: &str = "load_quick_save_";
const MUTE_ITEM_ID: &str = "mute";
const VOLUME_UP_ITEM_ID: &str = "volume_up";
const VOLUME_DOWN_ITEM_ID: &str = "volume_down";
const TOGGLE_HPF_ITEM_ID: &str = "toggle_hpf";
const TOGGLE_AUDIO_CHANNEL_ITEM_ID_PREFIX: &str = "toggle_audio_channel_";
const START_DEBUGGING_ITEM_ID: &str = "start_debugging";
const OPEN_VRAM_VIEW_ITEM_ID: &str = "open_vram_view";
const SHOW_FPS_ITEM_ID: &str = "show_fps";
const RESIZE_TO_FIT_ITEM_ID: &str = "resize_to_fit";
const COLOR_PALETTE_GRAYSCALE_ITEM_ID: &str = "color_palette_grayscale";
const COLOR_PALETTE_GREEN_ITEM_ID: &str = "color_palette_green";

impl EmulatorShellApp {
    pub(super) fn handle_menu_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let item_id = event.id().as_ref();
            match item_id {
                QUIT_ITEM_ID => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                PAUSE_ITEM_ID => self.send_command(Command::TogglePause),
                SAVE_ITEM_ID => self.send_command(Command::Save),
                MUTE_ITEM_ID => self.send_command(Command::ToggleMute),
                VOLUME_UP_ITEM_ID => self.send_command(Command::VolumeUp),
                VOLUME_DOWN_ITEM_ID => self.send_command(Command::VolumeDown),
                TOGGLE_HPF_ITEM_ID => self.send_command(Command::ToggleHpf),
                RESIZE_TO_FIT_ITEM_ID => self.resize_to_fit(ctx),
                START_DEBUGGING_ITEM_ID => self.show_debugger_view(ctx),
                OPEN_VRAM_VIEW_ITEM_ID => self.show_vram_view(ctx),
                SHOW_FPS_ITEM_ID => self.toggle_show_fps(),
                COLOR_PALETTE_GRAYSCALE_ITEM_ID => {
                    self.set_color_palette(ScreenColorPalette::Grayscale);
                }
                COLOR_PALETTE_GREEN_ITEM_ID => {
                    self.set_color_palette(ScreenColorPalette::Green);
                }
                _ => {
                    if let Some(slot_number) = item_id.strip_prefix(QUICK_SAVE_ITEM_ID_PREFIX) {
                        let slot = usize::from_str(slot_number).unwrap();
                        self.send_command(Command::QuickSave(slot));
                    }

                    if let Some(slot_number) = item_id.strip_prefix(LOAD_QUICK_SAVE_ITEM_ID_PREFIX)
                    {
                        let slot = usize::from_str(slot_number).unwrap();
                        self.send_command(Command::LoadQuickSave(slot));
                    }

                    if let Some(channel_number) =
                        item_id.strip_prefix(TOGGLE_AUDIO_CHANNEL_ITEM_ID_PREFIX)
                    {
                        let channel = usize::from_str(channel_number).unwrap();
                        self.send_command(Command::ToggleAudioChannel(channel));
                    }
                }
            }
        }
    }

    pub(super) fn update_color_palette_menu(&self, scren_palette: ScreenColorPalette) {
        let grayscale_menu_item =
            find_check_menu_item(self.menu(), COLOR_PALETTE_GRAYSCALE_ITEM_ID);
        let green_menu_item = find_check_menu_item(self.menu(), COLOR_PALETTE_GREEN_ITEM_ID);

        grayscale_menu_item.set_checked(matches!(scren_palette, ScreenColorPalette::Grayscale));
        green_menu_item.set_checked(matches!(scren_palette, ScreenColorPalette::Green));
    }
}

fn app_name_menu() -> Submenu {
    Submenu::with_id_and_items(
        APP_NAME_SUBMENU_ID,
        "GBC Emulator",
        true,
        &[&MenuItem::with_id(
            QUIT_ITEM_ID,
            "Quit GBC Emulator",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyQ)),
        )],
    )
    .unwrap()
}

fn emulator_menu() -> Submenu {
    let quick_save_submenu = Submenu::with_id(QUICK_SAVE_SUBMENU_ID, "Quick Save", true);
    let load_quick_save_submenu =
        Submenu::with_id(LOAD_QUICK_SAVE_SUBMENU_ID, "Load Quick Save", true);

    for i in 0..NUM_QUICK_SAVE_SLOTS {
        quick_save_submenu
            .append(&MenuItem::with_id(
                format!("{QUICK_SAVE_ITEM_ID_PREFIX}{i}"),
                format!("Save {i}"),
                true,
                Some(Accelerator::new(
                    Some(Modifiers::META),
                    Code::from_str(&format!("Digit{i}")).unwrap(),
                )),
            ))
            .unwrap();

        load_quick_save_submenu
            .append(&MenuItem::with_id(
                format!("{LOAD_QUICK_SAVE_ITEM_ID_PREFIX}{i}"),
                format!("Save {i}"),
                true,
                Some(Accelerator::new(
                    Some(Modifiers::META | Modifiers::SHIFT),
                    Code::from_str(&format!("Digit{i}")).unwrap(),
                )),
            ))
            .unwrap();
    }

    let color_palette_submenu = Submenu::with_id_and_items(
        COLOR_PALETTE_SUBMENU_ID,
        "Color Palette",
        true,
        &[
            &CheckMenuItem::with_id(
                COLOR_PALETTE_GRAYSCALE_ITEM_ID,
                "Grayscale",
                true,
                true,
                None,
            ),
            &CheckMenuItem::with_id(COLOR_PALETTE_GREEN_ITEM_ID, "Green", true, false, None),
        ],
    )
    .unwrap();

    Submenu::with_id_and_items(
        EMULATOR_SUBMENU_ID,
        "Emulator",
        true,
        &[
            &CheckMenuItem::with_id(
                PAUSE_ITEM_ID,
                "Pause",
                true,
                false,
                Some(Accelerator::new(Some(Modifiers::META), Code::KeyP)),
            ),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(
                SAVE_ITEM_ID,
                "Save",
                true,
                Some(Accelerator::new(Some(Modifiers::META), Code::KeyS)),
            ),
            &quick_save_submenu,
            &load_quick_save_submenu,
            &PredefinedMenuItem::separator(),
            &color_palette_submenu,
        ],
    )
    .unwrap()
}

fn audio_menu() -> Submenu {
    let audio_debug_submenu = Submenu::with_id(AUDIO_DEBUG_SUBMENU_ID, "Debug", true);

    for i in 0..NUM_AUDIO_CHANNELS {
        let channel = i + 1;
        audio_debug_submenu
            .append(&CheckMenuItem::with_id(
                format!("{TOGGLE_AUDIO_CHANNEL_ITEM_ID_PREFIX}{channel}"),
                format!("Channel {channel}"),
                true,
                true,
                Some(Accelerator::new(
                    Some(Modifiers::ALT | Modifiers::META),
                    Code::from_str(&format!("Digit{channel}")).unwrap(),
                )),
            ))
            .unwrap();
    }

    audio_debug_submenu
        .append(&PredefinedMenuItem::separator())
        .unwrap();

    audio_debug_submenu
        .append(&CheckMenuItem::with_id(
            TOGGLE_HPF_ITEM_ID,
            "High-Pass Filter",
            true,
            true,
            None,
        ))
        .unwrap();

    Submenu::with_id_and_items(
        AUDIO_SUBMENU_ID,
        "Audio",
        true,
        &[
            &MenuItem::with_id(
                MUTE_ITEM_ID,
                "Mute",
                true,
                Some(Accelerator::new(Some(Modifiers::META), Code::KeyM)),
            ),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(
                VOLUME_UP_ITEM_ID,
                "Volume Up",
                true,
                Some(Accelerator::new(Some(Modifiers::META), Code::Equal)),
            ),
            &MenuItem::with_id(
                VOLUME_DOWN_ITEM_ID,
                "Volume Down",
                true,
                Some(Accelerator::new(Some(Modifiers::META), Code::Minus)),
            ),
            &PredefinedMenuItem::separator(),
            &audio_debug_submenu,
        ],
    )
    .unwrap()
}

fn debug_menu() -> Submenu {
    Submenu::with_id_and_items(
        DEBUG_SUBMENU_ID,
        "Debug",
        true,
        &[
            &MenuItem::with_id(
                START_DEBUGGING_ITEM_ID,
                "Start Debugging",
                true,
                Some(Accelerator::new(Some(Modifiers::META), Code::KeyD)),
            ),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(OPEN_VRAM_VIEW_ITEM_ID, "Open VRAM View", true, None),
            &CheckMenuItem::with_id(SHOW_FPS_ITEM_ID, "Show FPS", true, false, None),
        ],
    )
    .unwrap()
}

fn window_menu() -> Submenu {
    Submenu::with_id_and_items(
        WINDOW_SUBMENU_ID,
        "Window",
        true,
        &[&MenuItem::with_id(
            RESIZE_TO_FIT_ITEM_ID,
            "Resize to Fit",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyF)),
        )],
    )
    .unwrap()
}

fn find_menu_item(menu: &Menu, id: &str) -> Option<MenuItemKind> {
    find_in_items(menu.items(), id)
}

fn find_check_menu_item(menu: &Menu, id: &str) -> CheckMenuItem {
    match find_menu_item(menu, id) {
        Some(MenuItemKind::Check(item)) => item,
        _ => panic!("CheckMenuItem with id '{}' not found", id),
    }
}

/// Recursively search the menu tree for the item with the given ID.
fn find_in_items(items: Vec<MenuItemKind>, id: &str) -> Option<MenuItemKind> {
    for item in items {
        if item.id().as_ref() == id {
            return Some(item);
        }
        if let MenuItemKind::Submenu(ref submenu) = item {
            if let Some(found) = find_in_items(submenu.items(), id) {
                return Some(found);
            }
        }
    }

    None
}

pub fn create_app_menu() -> Menu {
    let menu = Menu::new();
    menu.append(&app_name_menu()).unwrap();
    menu.append(&emulator_menu()).unwrap();
    menu.append(&audio_menu()).unwrap();
    menu.append(&debug_menu()).unwrap();
    menu.append(&window_menu()).unwrap();

    #[cfg(target_os = "macos")]
    menu.init_for_nsapp();

    menu
}
