use clap::Parser;

#[derive(Parser)]
#[command(about)]
pub struct Args {
    /// Print info about the ROM to stdout
    #[arg(long, default_value_t = false)]
    pub dump_rom_info: bool,

    /// Log information about each frame to stdout
    #[arg(long, default_value_t = false)]
    pub log_frames: bool,

    /// Run in headless mode (no GUI)
    #[arg(long, default_value_t = false)]
    pub headless: bool,

    /// ROM file to run
    #[arg(required = true)]
    pub rom: String,
}

pub struct Options {
    pub log_frames: bool,
}

impl Options {
    pub fn from_args(args: &Args) -> Self {
        Options {
            log_frames: args.log_frames,
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Options { log_frames: false }
    }
}
