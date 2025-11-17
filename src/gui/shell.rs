use std::{sync::mpsc::Sender, time::Duration};

use eframe::{
    egui::{self, Align2, Color32, FontId, Key, Pos2, Rect, Vec2, ViewportCommand},
    epaint::CornerRadius,
};
use muda::Menu;

use crate::{
    emulator::{Button, Command, SCREEN_HEIGHT, SCREEN_WIDTH, SharedOutputBuffer},
    gui::menu::create_app_menu,
};

const DEFAULT_SCALE_FACTOR: f32 = 4.0;

pub fn start_emulator_shell_app(commands_tx: Sender<Command>, output_buffer: SharedOutputBuffer) {
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
        Box::new(|_| Ok(Box::new(EmulatorShellApp::new(commands_tx, output_buffer)))),
    )
    .unwrap()
}

/// Target frames per second for the GUI to refresh
const GUI_FPS: f64 = 60.0;

pub struct EmulatorShellApp {
    /// Channel to send commands to the emulator
    commands_tx: Sender<Command>,

    /// Output from the emulator shared across threads
    shared_output: SharedOutputBuffer,

    /// Set of buttons that were pressed last frame
    pressed_buttons: u8,

    /// Whether we are currently in turbo mode, speeding up the emulation
    in_turbo_mode: bool,

    /// Whether the FPS counter should be shown onscreen
    show_fps: bool,

    /// The app menu. Must be kept alive for the menu to function.
    _menu: Menu,
}

impl EmulatorShellApp {
    fn new(commands_tx: Sender<Command>, shared_output: SharedOutputBuffer) -> Self {
        let menu = create_app_menu();

        Self {
            commands_tx,
            shared_output,
            pressed_buttons: 0,
            in_turbo_mode: false,
            show_fps: false,
            _menu: menu,
        }
    }

    pub fn send_command(&self, command: Command) {
        self.commands_tx.send(command).unwrap();
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
            self.send_command(Command::UpdatePressedButtons(buttons));
        }
    }

    fn handle_turbo_mode(&mut self, ctx: &egui::Context) {
        let in_turbo_mode = ctx.input(|i| i.key_down(Key::Space));
        if in_turbo_mode != self.in_turbo_mode {
            self.in_turbo_mode = in_turbo_mode;
            self.send_command(Command::SetTurboMode(in_turbo_mode));
        }
    }

    fn draw(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_screen(ui);

            if self.show_fps {
                self.draw_frame_rate_counter(ui);
            }
        });
    }

    fn draw_screen(&self, ui: &mut egui::Ui) {
        let scale_factor = self.calculate_scale_factor(ui.ctx());
        let painter = ui.painter();

        for y in 0..SCREEN_HEIGHT {
            for x in 0..SCREEN_WIDTH {
                let color = self.shared_output.read_pixel(x, y);
                painter.rect_filled(
                    rect_for_coordinate(x, y, scale_factor),
                    CornerRadius::ZERO,
                    color,
                );
            }
        }
    }

    fn draw_frame_rate_counter(&self, ui: &mut egui::Ui) {
        let fps = self.shared_output.frame_rate();

        ui.painter().text(
            Pos2::new(4.0, 4.0),
            Align2::LEFT_TOP,
            fps.to_string(),
            FontId::monospace(24.0),
            FPS_COUNTER_COLOR,
        );
    }

    fn calculate_scale_factor(&self, ctx: &egui::Context) -> f32 {
        let viewport_rect = ctx.viewport_rect();

        let width_scale = viewport_rect.width() / (SCREEN_WIDTH as f32);
        let height_scale = viewport_rect.height() / (SCREEN_HEIGHT as f32);

        width_scale.min(height_scale)
    }

    pub fn resize_to_fit(&self, ctx: &egui::Context) {
        let scale_factor = self.calculate_scale_factor(ctx);

        let new_size = Vec2::new(
            scale_factor * (SCREEN_WIDTH as f32),
            scale_factor * (SCREEN_HEIGHT as f32),
        );

        ctx.send_viewport_cmd(ViewportCommand::InnerSize(new_size));
    }

    pub fn toggle_show_fps(&mut self) {
        self.show_fps = !self.show_fps;
    }
}

impl eframe::App for EmulatorShellApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_secs_f64(1.0 / GUI_FPS));

        self.handle_menu_events(ctx);
        self.handle_pressed_buttons(ctx);
        self.handle_turbo_mode(ctx);

        self.draw(ctx);
    }
}

const FPS_COUNTER_COLOR: Color32 = Color32::from_rgba_unmultiplied_const(0, 0, 255, 128);

fn rect_for_coordinate(x: usize, y: usize, scale_factor: f32) -> Rect {
    Rect::from_x_y_ranges(
        ((x as f32) * scale_factor)..=(((x as f32) + 1.0) * scale_factor),
        ((y as f32) * scale_factor)..=(((y as f32) + 1.0) * scale_factor),
    )
}
