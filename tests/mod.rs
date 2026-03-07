mod utils;

use gbcemu::{emulator::EmulatorBuilder, machine::Machine};
use utils::{
    assert_emulator_matches_image, read_cartridge_file, read_image_file, resolve_fixture_path,
    run_emulator_for_n_frames,
};
fn run_dmg_acid2_test(machine: Machine, reference_filename: &str) {
    const DMG_GRAYSCALE_PALETTE: [image::Rgb<u8>; 4] = [
        image::Rgb([0xFF, 0xFF, 0xFF]),
        image::Rgb([0xAA, 0xAA, 0xAA]),
        image::Rgb([0x55, 0x55, 0x55]),
        image::Rgb([0x00, 0x00, 0x00]),
    ];
    const NUM_FRAMES: usize = 10;

    let rom_path = resolve_fixture_path("dmg-acid2.gb");
    let cartridge = read_cartridge_file(&rom_path);
    let mut emulator = EmulatorBuilder::new_cartridge(cartridge, machine).build();

    run_emulator_for_n_frames(&mut emulator, NUM_FRAMES);

    let reference_path = resolve_fixture_path(reference_filename);
    let expected_image = read_image_file(&reference_path);

    assert_emulator_matches_image(&emulator, &expected_image, DMG_GRAYSCALE_PALETTE);
}

#[test]
fn dmg_acid2() {
    run_dmg_acid2_test(Machine::Dmg, "dmg-acid2-reference-dmg.png");
}
