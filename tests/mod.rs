mod utils;

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

fn run_dmg_acid2_test(machine: Machine, reference_filename: &str) {
    let rom_path = resolve_gameboy_test_roms_path("dmg-acid2/dmg-acid2.gb");
    let cartridge = read_cartridge_file(&rom_path);
    let mut emulator = EmulatorBuilder::new_cartridge(cartridge, machine).build();

    run_emulator_for_n_frames(&mut emulator, 10);

    let reference_path = resolve_gameboy_test_roms_path(reference_filename);
    let expected_image = read_image_file(&reference_path);

    assert_emulator_matches_image(&emulator, &expected_image, DMG_GRAYSCALE_PALETTE);
}

#[test]
fn dmg_acid2() {
    run_dmg_acid2_test(Machine::Dmg, "dmg-acid2/dmg-acid2-dmg.png");
}

#[test]
fn cgb_acid2() {
    let rom_path = resolve_gameboy_test_roms_path("cgb-acid2/cgb-acid2.gbc");
    let cartridge = read_cartridge_file(&rom_path);
    let mut emulator = EmulatorBuilder::new_cartridge(cartridge, Machine::Cgb).build();

    run_emulator_for_n_frames(&mut emulator, 100);

    let reference_path = resolve_gameboy_test_roms_path("cgb-acid2/cgb-acid2.png");
    let expected_image = read_image_file(&reference_path);

    assert_emulator_matches_image(&emulator, &expected_image, DMG_GRAYSCALE_PALETTE);
}

fn run_blargg_test(rom_path_in_repo: &str, expected: &str, max_frames: usize) {
    let rom_path = resolve_blarggs_path(rom_path_in_repo);
    let cartridge = read_cartridge_file(&rom_path);
    let mut emulator = EmulatorBuilder::new_cartridge(cartridge, Machine::Dmg).build();

    run_emulator_for_n_frames(&mut emulator, max_frames);

    let expected_path = resolve_blarggs_path(expected);
    let expected_image = read_image_file(&expected_path);

    assert_emulator_matches_image(&emulator, &expected_image, DMG_GRAYSCALE_PALETTE);
}

#[test]
fn blarggs_cpu_instrs() {
    run_blargg_test(
        "cpu_instrs/cpu_instrs.gb",
        "cpu_instrs/cpu_instrs-dmg-cgb.png",
        4_000,
    );
}

#[test]
fn blarggs_instr_timing() {
    run_blargg_test(
        "instr_timing/instr_timing.gb",
        "instr_timing/instr_timing-dmg-cgb.png",
        100,
    );
}
