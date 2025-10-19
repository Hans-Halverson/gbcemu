use clap::Parser;

use gbcemu::rom::Rom;

#[derive(Parser)]
#[command(about)]
struct Args {
    /// ROM file to run
    #[arg(required = true)]
    rom: String,
}

fn main() {
    let args = Args::parse();

    let rom_bytes = read_file(&args.rom);
    let rom = Rom::new_from_bytes(rom_bytes);

    println!("{:?}", rom);
}

fn read_file(path: &str) -> Vec<u8> {
    std::fs::read(path).expect("Failed to read file")
}
