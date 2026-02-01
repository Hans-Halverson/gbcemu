use std::str::FromStr;

use eframe::egui;
use muda::{
    CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
    accelerator::{Accelerator, Code, Modifiers},
};

use crate::{
    audio::NUM_AUDIO_CHANNELS, emulator::Command, gui::shell::EmulatorShellApp,
    save_file::NUM_QUICK_SAVE_SLOTS,
};

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
const OPEN_VRAM_VIEW_ITEM_ID: &str = "open_vram_view";
const SHOW_FPS_ITEM_ID: &str = "show_fps";
const RESIZE_TO_FIT_ITEM_ID: &str = "resize_to_fit";

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
                OPEN_VRAM_VIEW_ITEM_ID => self.open_vram_view(),
                SHOW_FPS_ITEM_ID => self.toggle_show_fps(),
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
}

fn app_name_menu() -> Submenu {
    Submenu::with_items(
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
    let quick_save_submenu = Submenu::new("Quick Save", true);
    let load_quick_save_submenu = Submenu::new("Load Quick Save", true);

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

    Submenu::with_items(
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
        ],
    )
    .unwrap()
}

fn audio_menu() -> Submenu {
    let audio_debug_submenu = Submenu::new("Debug", true);

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

    Submenu::with_items(
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
    Submenu::with_items(
        "Debug",
        true,
        &[
            &MenuItem::with_id(OPEN_VRAM_VIEW_ITEM_ID, "Open VRAM View", true, None),
            &CheckMenuItem::with_id(SHOW_FPS_ITEM_ID, "Show FPS", true, false, None),
        ],
    )
    .unwrap()
}

fn window_menu() -> Submenu {
    Submenu::with_items(
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
