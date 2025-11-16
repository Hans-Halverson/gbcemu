use clap::Parser;
use gbcemu::{
    audio::DefaultSystemAudioOutput,
    cartridge::Cartridge,
    emulator::{EmulatorBuilder, SharedInputAdapter, SharedOutputBuffer},
    gui::shell::start_emulator_shell_app,
    machine::Machine,
    options::{Args, Options},
    save_file::SAVE_FILE_EXTENSION,
};

use std::{
    fs,
    sync::{Arc, mpsc::channel},
    thread::{self, JoinHandle},
};

// The only supported ROM file extensions
const GB_FILE_EXTENSION: &str = ".gb";
const GBC_FILE_EXTENSION: &str = ".gbc";

fn main() {
    let args = Args::parse();
    let options = Arc::new(Options::from_args(&args));

    let (commands_tx, commands_rx) = channel();

    let input_adapter = SharedInputAdapter::new(commands_rx);
    let output_buffer = SharedOutputBuffer::new();

    let emulator_thread =
        start_emulator_thread(&args, options.clone(), input_adapter, output_buffer.clone());

    if !args.headless && !args.dump_rom_info {
        start_emulator_shell_app(commands_tx, output_buffer);
    } else {
        emulator_thread.join().unwrap();
    }
}

fn start_emulator_thread(
    args: &Args,
    options: Arc<Options>,
    input_adapter: SharedInputAdapter,
    output_buffer: SharedOutputBuffer,
) -> JoinHandle<()> {
    let machine = if args.cgb { Machine::Cgb } else { Machine::Dmg };
    let rom_or_save_path = args.rom_or_save.clone();
    let dump_rom_info = args.dump_rom_info;
    let bios_path = args.bios.clone();

    spawn_emulator_thread(move || {
        let mut emulator_builder = if rom_or_save_path.ends_with(SAVE_FILE_EXTENSION) {
            let save_file_bytes = fs::read(&rom_or_save_path).expect("Failed to read save file");
            let save_file = rmp_serde::from_slice(&save_file_bytes)
                .expect("Could not read save file, save file format may have changed");

            EmulatorBuilder::from_saved_cartidge(save_file, machine)
                .with_save_file_path(rom_or_save_path)
        } else if rom_or_save_path.ends_with(GB_FILE_EXTENSION)
            || rom_or_save_path.ends_with(GBC_FILE_EXTENSION)
        {
            let rom_bytes = fs::read(&rom_or_save_path).expect("Failed to read ROM");
            let cartridge = Cartridge::new_from_rom_bytes(rom_bytes);

            let save_file_path = rom_or_save_path
                .trim_end_matches(GB_FILE_EXTENSION)
                .trim_end_matches(GBC_FILE_EXTENSION)
                .to_string()
                + SAVE_FILE_EXTENSION;

            EmulatorBuilder::new_cartridge(cartridge, machine).with_save_file_path(save_file_path)
        } else {
            panic!(
                "Unsupported file type, file must have {}, {}, or {} extension",
                GB_FILE_EXTENSION, GBC_FILE_EXTENSION, SAVE_FILE_EXTENSION
            );
        };

        emulator_builder = emulator_builder
            .with_options(options)
            .with_input_adapter(input_adapter)
            .with_output_buffer(output_buffer)
            .with_audio_output(Box::new(DefaultSystemAudioOutput::new()));

        if let Some(bios_path) = bios_path {
            emulator_builder = emulator_builder.with_bios_path(bios_path);
        }

        let mut emulator = emulator_builder.build();

        if dump_rom_info {
            println!("{:?}", emulator.cartridge());
            return;
        }

        emulator.run();
    })
}

fn spawn_emulator_thread(f: impl FnOnce() + Send + 'static) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("emulator".to_string())
        .spawn(f)
        .unwrap()
}
