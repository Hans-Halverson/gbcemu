use std::path::{Path, PathBuf};

use eframe::egui::{self};
use image;

use gbcemu::{
    cartridge::Cartridge,
    emulator::{Emulator, SCREEN_HEIGHT, SCREEN_WIDTH},
    ppu::Color,
};

/// Read a ROM file into a Cartridge.
pub fn read_cartridge_file(rom_path: &Path) -> Cartridge {
    let rom_bytes = std::fs::read(&rom_path).unwrap_or_else(|_| {
        panic!(
            "ROM not found at {}. Run install_test_dependencies.sh first.",
            rom_path.to_string_lossy()
        )
    });

    Cartridge::new_from_rom_bytes(rom_bytes)
}

/// Read an image file
pub fn read_image_file(img_path: &Path) -> image::RgbImage {
    image::open(&img_path)
        .unwrap_or_else(|_| {
            panic!(
                "Image not found at {}. Run install_test_dependencies.sh first.",
                img_path.to_string_lossy()
            )
        })
        .to_rgb8()
}

/// Run a ROM for a specified number of frames and return the resulting emulator state.
pub fn run_emulator_for_n_frames(emulator: &mut Emulator, num_frames: usize) {
    emulator.emulate_boot_sequence();

    for _ in 0..num_frames {
        emulator.run_frame();
    }
}

pub fn resolve_fixture_path(filename: &str) -> PathBuf {
    Path::new("deps").join("test").join(filename)
}

pub fn assert_emulator_matches_image(
    emulator: &Emulator,
    reference_img: &image::RgbImage,
    palette: [image::Rgb<u8>; 4],
) {
    for y in 0..SCREEN_HEIGHT {
        for x in 0..SCREEN_WIDTH {
            let actual_pixel = match emulator.read_pixel(x, y) {
                Color::Dmg(idx) => palette[idx as usize],
                Color::Cgb(cgb) => egui_color32_to_rgb8(cgb.to_color32()),
            };
            let expected_pixel = *reference_img.get_pixel(x as u32, y as u32);

            assert_eq!(
                actual_pixel, expected_pixel,
                "pixel mismatch at ({x}, {y}): emulator={:?} reference={:?}",
                actual_pixel, expected_pixel
            );
        }
    }
}

pub fn egui_color32_to_rgb8(color32: egui::Color32) -> image::Rgb<u8> {
    image::Rgb([color32.r(), color32.g(), color32.b()])
}
