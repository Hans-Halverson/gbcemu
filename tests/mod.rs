mod utils;

use std::path::Path;

use gbcemu::{emulator::EmulatorBuilder, machine::Machine};
use utils::{
    assert_emulator_matches_image, read_cartridge_file, read_image_file, resolve_blarggs_path,
    run_emulator_for_n_frames,
};

use crate::utils::resolve_gameboy_test_roms_path;

const DMG_GRAYSCALE_PALETTE: [image::Rgb<u8>; 4] = [
    image::Rgb([0xFF, 0xFF, 0xFF]),
    image::Rgb([0xAA, 0xAA, 0xAA]),
    image::Rgb([0x55, 0x55, 0x55]),
    image::Rgb([0x00, 0x00, 0x00]),
];

fn run_screenshot_test(
    rom_path: &Path,
    image_path: &Path,
    machine: Machine,
    num_frames_to_run: usize,
) {
    let cartridge = read_cartridge_file(rom_path);
    let mut emulator = EmulatorBuilder::new_cartridge(cartridge, machine).build();

    run_emulator_for_n_frames(&mut emulator, num_frames_to_run);

    let expected_image = read_image_file(image_path);

    assert_emulator_matches_image(&emulator, &expected_image, DMG_GRAYSCALE_PALETTE);
}

#[test]
fn dmg_acid2() {
    run_screenshot_test(
        &resolve_gameboy_test_roms_path("dmg-acid2/dmg-acid2.gb"),
        &resolve_gameboy_test_roms_path("dmg-acid2/dmg-acid2-dmg.png"),
        Machine::Dmg,
        10,
    );
}

#[test]
fn cgb_acid2() {
    run_screenshot_test(
        &resolve_gameboy_test_roms_path("cgb-acid2/cgb-acid2.gbc"),
        &resolve_gameboy_test_roms_path("cgb-acid2/cgb-acid2.png"),
        Machine::Cgb,
        100,
    );
}

#[test]
fn blarggs_cpu_instrs() {
    run_screenshot_test(
        &resolve_blarggs_path("cpu_instrs/cpu_instrs.gb"),
        &resolve_blarggs_path("cpu_instrs/cpu_instrs-dmg-cgb.png"),
        Machine::Dmg,
        4_000,
    );
}

#[test]
fn blarggs_instr_timing() {
    run_screenshot_test(
        &resolve_blarggs_path("instr_timing/instr_timing.gb"),
        &resolve_blarggs_path("instr_timing/instr_timing-dmg-cgb.png"),
        Machine::Dmg,
        100,
    );
}

#[test]
fn mbc3_tester() {
    run_screenshot_test(
        &resolve_gameboy_test_roms_path("mbc3-tester/mbc3-tester.gb"),
        &resolve_gameboy_test_roms_path("mbc3-tester/mbc3-tester-dmg.png"),
        Machine::Dmg,
        40,
    );

    run_screenshot_test(
        &resolve_gameboy_test_roms_path("mbc3-tester/mbc3-tester.gb"),
        &resolve_gameboy_test_roms_path("mbc3-tester/mbc3-tester-dmg.png"),
        Machine::Cgb,
        40,
    );
}
