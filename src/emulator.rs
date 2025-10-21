use std::{
    array,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use eframe::egui::Color32;

/// Width of the gameboy screen in pixels
pub const SCREEN_WIDTH: usize = 160;

/// Height of the gameboy screen in pixels
pub const SCREEN_HEIGHT: usize = 144;

/// A reference to a shared output buffer.
#[derive(Clone)]
pub struct SharedOutputBuffer {
    pixels: Arc<[[AtomicU32; SCREEN_WIDTH]; SCREEN_HEIGHT]>,
}

impl SharedOutputBuffer {
    pub fn new() -> Self {
        let pixels = Arc::new(
            array::from_fn::<[AtomicU32; SCREEN_WIDTH], SCREEN_HEIGHT, _>(|_| {
                array::from_fn::<AtomicU32, SCREEN_WIDTH, _>(|_| AtomicU32::new(0xFFFFFFFF))
            }),
        );

        Self { pixels }
    }

    pub fn read_pixel(&self, x: usize, y: usize) -> Color32 {
        let encoded = self.pixels[y][x].load(Ordering::Relaxed);

        let r = (encoded >> 24) as u8;
        let g = (encoded >> 16) as u8;
        let b = (encoded >> 8) as u8;
        let a = encoded as u8;

        Color32::from_rgba_premultiplied(r, g, b, a)
    }

    pub fn write_pixel(&self, x: usize, y: usize, color: Color32) {
        let encoded = ((color.r() as u32) << 24)
            | ((color.g() as u32) << 16)
            | ((color.b() as u32) << 8)
            | (color.a() as u32);

        self.pixels[y][x].store(encoded, Ordering::Relaxed);
    }
}

pub struct Emulator {
    output_buffer: SharedOutputBuffer,
}

impl Emulator {
    pub fn new() -> Self {
        Emulator {
            output_buffer: SharedOutputBuffer::new(),
        }
    }

    pub fn write_pixel(&self, x: usize, y: usize, color: Color32) {
        self.output_buffer.write_pixel(x, y, color);
    }

    pub fn clone_output_buffer(&self) -> SharedOutputBuffer {
        self.output_buffer.clone()
    }

    pub fn run(&mut self) {}
}
