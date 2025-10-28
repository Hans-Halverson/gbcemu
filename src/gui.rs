use std::time::Duration;

use eframe::{
    egui::{self, Key, Rect},
    epaint::CornerRadius,
};

use crate::emulator::{
    Button, SCREEN_HEIGHT, SCREEN_WIDTH, SharedInputAdapter, SharedOutputBuffer,
};

/// Size in pixels of a single gb pixel
const PIXEL_SCALE: usize = 4;

pub fn start_gui(input_adapter: SharedInputAdapter, output_buffer: SharedOutputBuffer) {
    eframe::run_native(
        "GBC Emulator",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([
                    (SCREEN_WIDTH * PIXEL_SCALE) as f32,
                    (SCREEN_HEIGHT * PIXEL_SCALE) as f32,
                ])
                .with_active(true)
                .with_resizable(false)
                .with_title_shown(true),
            ..Default::default()
        },
        Box::new(|_| {
            Ok(Box::new(GuiApp {
                input_adapter,
                pixels: output_buffer,
            }))
        }),
    )
    .unwrap()
}

/// Target frames per second for the GUI to refresh
const GUI_FPS: f64 = 60.0;

struct GuiApp {
    input_adapter: SharedInputAdapter,
    pixels: SharedOutputBuffer,
}

impl GuiApp {}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_secs_f64(1.0 / GUI_FPS));

        egui::CentralPanel::default().show(ctx, |ui| {
            let painter = ui.painter();

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

            self.input_adapter.set_pressed_buttons(buttons);

            for y in 0..SCREEN_HEIGHT {
                for x in 0..SCREEN_WIDTH {
                    let color = self.pixels.read_pixel(x, y);
                    painter.rect_filled(
                        Rect::from_x_y_ranges(
                            ((x * PIXEL_SCALE) as f32)..=((x + 1) * PIXEL_SCALE) as f32,
                            ((y * PIXEL_SCALE) as f32)..=((y + 1) * PIXEL_SCALE) as f32,
                        ),
                        CornerRadius::ZERO,
                        color,
                    );
                }
            }
        });
    }
}
