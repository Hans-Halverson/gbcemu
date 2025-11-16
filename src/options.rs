use clap::Parser;

#[derive(Parser)]
#[command(about)]
pub struct Args {
    /// Print info about the ROM to stdout
    #[arg(long, default_value_t = false)]
    pub dump_rom_info: bool,

    /// Emulate a GameBoy Color instead of a regular GameBoy
    #[arg(long, default_value_t = false)]
    pub cgb: bool,

    /// Log information about each frame to stdout
    #[arg(long, default_value_t = false)]
    pub log_frames: bool,

    /// Run in headless mode (no GUI)
    #[arg(long, default_value_t = false)]
    pub headless: bool,

    /// Run in test mode (print success or failure based on magic instruction)
    #[arg(long, default_value_t = false)]
    pub test: bool,

    /// Path to the boot ROM to use
    #[arg(long)]
    pub bios: Option<String>,

    /// ROM or save file to run
    #[arg(required = true)]
    pub rom_or_save: String,
}

pub struct Options {
    pub log_frames: bool,
    pub in_test_mode: bool,
}

impl Options {
    pub fn from_args(args: &Args) -> Self {
        Options {
            log_frames: args.log_frames,
            in_test_mode: args.test,
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Options {
            log_frames: false,
            in_test_mode: false,
        }
    }
}
