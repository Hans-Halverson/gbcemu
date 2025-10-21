use clap::Parser;

use gbcemu::{emulator::Emulator, rom::Rom};

#[derive(Parser)]
#[command(about)]
struct Args {
    /// Print info about the ROM to stdout
    #[arg(long, default_value_t = false)]
    dump_rom: bool,

    /// ROM file to run
    #[arg(required = true)]
    rom: String,
}

fn main() {
    let args = Args::parse();

    let rom_bytes = read_file(&args.rom);
    let rom = Rom::new_from_bytes(rom_bytes);

    if args.dump_rom {
        println!("{:?}", rom);
        return;
    }

    let emulator = Box::new(Emulator::new());

    gbcemu::gui::start_gui_app(emulator).unwrap();
}

fn read_file(path: &str) -> Vec<u8> {
    std::fs::read(path).expect("Failed to read file")
}
