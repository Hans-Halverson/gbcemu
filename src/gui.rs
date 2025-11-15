use std::{str::FromStr, sync::mpsc::Sender, time::Duration};

use eframe::{
    egui::{self, Key, Rect, Vec2, ViewportCommand},
    epaint::CornerRadius,
};
use muda::{
    CheckMenuItem, Menu, MenuEvent, MenuItem, Submenu,
    accelerator::{Accelerator, Code, Modifiers},
};

use crate::{
    audio::NUM_AUDIO_CHANNELS,
    emulator::{Button, Command, SCREEN_HEIGHT, SCREEN_WIDTH, SharedOutputBuffer},
    save_file::NUM_QUICK_SAVE_SLOTS,
};

const DEFAULT_SCALE_FACTOR: f32 = 4.0;

pub fn start_gui(commands_tx: Sender<Command>, output_buffer: SharedOutputBuffer) {
    eframe::run_native(
        "GBC Emulator",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([
                    (SCREEN_WIDTH as f32) * DEFAULT_SCALE_FACTOR,
                    (SCREEN_HEIGHT as f32) * DEFAULT_SCALE_FACTOR,
                ])
                .with_min_inner_size([SCREEN_WIDTH as f32, SCREEN_HEIGHT as f32])
                .with_active(true)
                .with_title_shown(true),
            ..Default::default()
        },
        Box::new(|_| {
            let menu = create_app_menu();

            Ok(Box::new(GuiApp {
                commands_tx,
                pixels: output_buffer,
                pressed_buttons: 0,
                in_turbo_mode: false,
                _menu: menu,
            }))
        }),
    )
    .unwrap()
}

/// Target frames per second for the GUI to refresh
const GUI_FPS: f64 = 60.0;

struct GuiApp {
    commands_tx: Sender<Command>,
    pixels: SharedOutputBuffer,
    pressed_buttons: u8,
    in_turbo_mode: bool,
    /// The app menu. Must be kept alive for the menu to function.
    _menu: Menu,
}

impl GuiApp {
    fn handle_menu_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let item_id = event.id().as_ref();
            match item_id {
                QUIT_ITEM_ID => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                SAVE_ITEM_ID => self.commands_tx.send(Command::Save).unwrap(),
                RESIZE_TO_FIT_ITEM_ID => self.resize_to_fit(ctx),
                _ => {
                    if let Some(slot_number) = item_id.strip_prefix(QUICK_SAVE_ITEM_ID_PREFIX) {
                        let slot = usize::from_str(slot_number).unwrap();
                        self.commands_tx.send(Command::QuickSave(slot)).unwrap();
                    }

                    if let Some(slot_number) = item_id.strip_prefix(LOAD_QUICK_SAVE_ITEM_ID_PREFIX)
                    {
                        let slot = usize::from_str(slot_number).unwrap();
                        self.commands_tx.send(Command::LoadQuickSave(slot)).unwrap();
                    }

                    if let Some(channel_number) =
                        item_id.strip_prefix(TOGGLE_AUDIO_CHANNEL_ITEM_ID_PREFIX)
                    {
                        let channel = usize::from_str(channel_number).unwrap();
                        self.commands_tx
                            .send(Command::ToggleAudioChannel(channel))
                            .unwrap();
                    }
                }
            }
        }
    }

    fn handle_pressed_buttons(&mut self, ctx: &egui::Context) {
        let mut buttons = 0;
        if ctx.input(|i| i.key_down(Key::A)) {
            buttons |= Button::Select as u8;
        }
        if ctx.input(|i| i.key_down(Key::S)) {
            buttons |= Button::Start as u8;
        }
        if ctx.input(|i| i.key_down(Key::Z)) {
            buttons |= Button::B as u8;
        }
        if ctx.input(|i| i.key_down(Key::X)) {
            buttons |= Button::A as u8;
        }
        if ctx.input(|i| i.key_down(Key::ArrowUp)) {
            buttons |= Button::Up as u8;
        }
        if ctx.input(|i| i.key_down(Key::ArrowDown)) {
            buttons |= Button::Down as u8;
        }
        if ctx.input(|i| i.key_down(Key::ArrowLeft)) {
            buttons |= Button::Left as u8;
        }
        if ctx.input(|i| i.key_down(Key::ArrowRight)) {
            buttons |= Button::Right as u8;
        }

        if buttons != self.pressed_buttons {
            self.pressed_buttons = buttons;
            self.commands_tx
                .send(Command::UpdatePressedButtons(buttons))
                .unwrap();
        }
    }

    fn handle_turbo_mode(&mut self, ctx: &egui::Context) {
        let in_turbo_mode = ctx.input(|i| i.key_down(Key::Space));
        if in_turbo_mode != self.in_turbo_mode {
            self.in_turbo_mode = in_turbo_mode;
            self.commands_tx
                .send(Command::SetTurboMode(in_turbo_mode))
                .unwrap();
        }
    }

    fn draw_screen(&mut self, ctx: &egui::Context) {
        let scale_factor = self.calculate_scale_factor(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            let painter = ui.painter();

            for y in 0..SCREEN_HEIGHT {
                for x in 0..SCREEN_WIDTH {
                    let color = self.pixels.read_pixel(x, y);
                    painter.rect_filled(
                        Rect::from_x_y_ranges(
                            ((x as f32) * scale_factor)..=((x as f32 + 1.0) * scale_factor),
                            ((y as f32) * scale_factor)..=((y as f32 + 1.0) * scale_factor),
                        ),
                        CornerRadius::ZERO,
                        color,
                    );
                }
            }
        });
    }

    fn calculate_scale_factor(&self, ctx: &egui::Context) -> f32 {
        let viewport_rect = ctx.viewport_rect();

        let width_scale = viewport_rect.width() / (SCREEN_WIDTH as f32);
        let height_scale = viewport_rect.height() / (SCREEN_HEIGHT as f32);

        width_scale.min(height_scale)
    }

    fn resize_to_fit(&self, ctx: &egui::Context) {
        let scale_factor = self.calculate_scale_factor(ctx);

        let new_size = Vec2::new(
            scale_factor * (SCREEN_WIDTH as f32),
            scale_factor * (SCREEN_HEIGHT as f32),
        );

        ctx.send_viewport_cmd(ViewportCommand::InnerSize(new_size));
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_secs_f64(1.0 / GUI_FPS));

        self.handle_menu_events(ctx);
        self.handle_pressed_buttons(ctx);
        self.handle_turbo_mode(ctx);

        self.draw_screen(ctx);
    }
}

const QUIT_ITEM_ID: &str = "quit";
const SAVE_ITEM_ID: &str = "save";
const RESIZE_TO_FIT_ITEM_ID: &str = "resize_to_fit";
const QUICK_SAVE_ITEM_ID_PREFIX: &str = "quick_save_";
const LOAD_QUICK_SAVE_ITEM_ID_PREFIX: &str = "load_quick_save_";
const TOGGLE_AUDIO_CHANNEL_ITEM_ID_PREFIX: &str = "toggle_audio_channel_";

fn create_app_menu() -> Menu {
    let menu = Menu::new();

    let app_name_menu = Submenu::with_items(
        "GBC Emulator",
        true,
        &[&MenuItem::with_id(
            QUIT_ITEM_ID,
            "Quit GBC Emulator",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyQ)),
        )],
    )
    .unwrap();

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

    let emulator_menu = Submenu::with_items(
        "Emulator",
        true,
        &[
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
    .unwrap();

    let channels_submenu = Submenu::new("Channels", true);

    for i in 0..NUM_AUDIO_CHANNELS {
        let channel = i + 1;
        channels_submenu
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

    let audio_menu = Submenu::with_items("Audio", true, &[&channels_submenu]).unwrap();

    let window_menu = Submenu::with_items(
        "Window",
        true,
        &[&MenuItem::with_id(
            RESIZE_TO_FIT_ITEM_ID,
            "Resize to Fit",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyF)),
        )],
    )
    .unwrap();

    menu.append(&app_name_menu).unwrap();
    menu.append(&emulator_menu).unwrap();
    menu.append(&audio_menu).unwrap();
    menu.append(&window_menu).unwrap();

    #[cfg(target_os = "macos")]
    menu.init_for_nsapp();

    menu
}
