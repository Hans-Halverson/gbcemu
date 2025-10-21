use clap::Parser;
use gbcemu::{
    emulator::{Emulator, SharedOutputBuffer},
    gui::start_gui,
    options::{Args, Options},
    rom::Rom,
};

use std::{
    sync::{Arc, mpsc::channel},
    thread::{self, JoinHandle},
};

fn main() {
    let args = Args::parse();
    let options = Arc::new(Options::from_args(&args));

    let rom_bytes = read_file(&args.rom);
    let rom = Rom::new_from_bytes(rom_bytes);

    if args.dump_rom {
        println!("{:?}", rom);
        return;
    }

    let (emulator_thread, shared_output_buffer) = start_emulator_thread(options.clone());

    if !args.headless {
        start_gui(shared_output_buffer);
    } else {
        emulator_thread.join().unwrap();
    }
}

fn read_file(path: &str) -> Vec<u8> {
    std::fs::read(path).expect("Failed to read file")
}

/// Start the emulator in a separate thread and return a buffer where results can be written that
/// can be shared across threads.
fn start_emulator_thread(options: Arc<Options>) -> (JoinHandle<()>, SharedOutputBuffer) {
    let (sender, receiver) = channel();

    let emulator_thread = thread::spawn(move || {
        let mut emulator = Box::new(Emulator::new(options));
        sender.send(emulator.clone_output_buffer()).unwrap();
        emulator.run();
    });

    (emulator_thread, receiver.recv().unwrap())
}
