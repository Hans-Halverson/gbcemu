use std::thread;

use eframe::{
    egui::{self, Rect},
    epaint::CornerRadius,
};

use crate::emulator::{Emulator, SCREEN_HEIGHT, SCREEN_WIDTH, SharedOutputBuffer};

/// Size in pixels of a single gb pixel
const PIXEL_SCALE: usize = 4;

pub fn start_gui_app(mut emulator: Box<Emulator>) -> eframe::Result<()> {
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
            let output_buffer = emulator.clone_output_buffer();

            thread::spawn(move || emulator.run());

            Ok(Box::new(GuiApp {
                pixels: output_buffer,
            }))
        }),
    )
}

struct GuiApp {
    pixels: SharedOutputBuffer,
}

impl GuiApp {}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let painter = ui.painter();

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
