use std::{
    array,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use eframe::egui::Color32;

use crate::{
    address_space::{Address, EXTERNAL_RAM_END, ROM_END, VRAM_END},
    cartridge::Cartridge,
    mbc::mbc::Location,
    options::Options,
};

/// Width of the gameboy screen in pixels
pub const SCREEN_WIDTH: usize = 160;

/// Height of the gameboy screen in pixels
pub const SCREEN_HEIGHT: usize = 144;

/// A reference to a shared output buffer.
#[derive(Clone)]
pub struct SharedOutputBuffer {
    pixels: Arc<[[AtomicU32; SCREEN_WIDTH]; SCREEN_HEIGHT]>,
}

impl SharedOutputBuffer {
    pub fn new() -> Self {
        let pixels = Arc::new(
            array::from_fn::<[AtomicU32; SCREEN_WIDTH], SCREEN_HEIGHT, _>(|_| {
                array::from_fn::<AtomicU32, SCREEN_WIDTH, _>(|_| AtomicU32::new(0xFFFFFFFF))
            }),
        );

        Self { pixels }
    }

    pub fn read_pixel(&self, x: usize, y: usize) -> Color32 {
        let encoded = self.pixels[y][x].load(Ordering::Relaxed);

        let r = (encoded >> 24) as u8;
        let g = (encoded >> 16) as u8;
        let b = (encoded >> 8) as u8;
        let a = encoded as u8;

        Color32::from_rgba_premultiplied(r, g, b, a)
    }

    pub fn write_pixel(&self, x: usize, y: usize, color: Color32) {
        let encoded = ((color.r() as u32) << 24)
            | ((color.g() as u32) << 16)
            | ((color.b() as u32) << 8)
            | (color.a() as u32);

        self.pixels[y][x].store(encoded, Ordering::Relaxed);
    }
}

/// Refresh rate of the GameBoy screen in Hz
const REFRESH_RATE: f64 = 59.7;

/// Total number of scanlines including VBlank period. Larger than the height of the screen.
const NUM_VIRTUAL_SCANLINES: usize = 154;

const TICKS_PER_FRAME: usize = 70224;

const TICKS_PER_SCANLINE: usize = TICKS_PER_FRAME / NUM_VIRTUAL_SCANLINES;

/// Number of ticks in OAM Scan mode at the beginning of each scanline
const OAM_SCAN_TICKS: usize = 80;

/// Nanoseconds in real time per frame
const NS_PER_FRAME: f64 = 1_000_000_000.0f64 / REFRESH_RATE;

enum Mode {
    /// Mode 0: Move to the next scanline
    HBlank,
    /// Mode 1: Move back to start of screen
    VBlank,
    /// Mode 2: Search OAM for sprites in current scanline
    OAMScan,
    /// Mode 3: Draw pixels to screen
    Draw,
}

pub struct Emulator {
    /// Cartridge inserted
    cartridge: Cartridge,

    /// Options for the emulator
    options: Arc<Options>,

    /// Output buffer for the screen which is shared with the GUI thread
    output_buffer: SharedOutputBuffer,

    /// Current tick (T-cycle) within a frame
    tick: u32,

    /// Current frame number
    frame: u64,

    /// Current mode
    mode: Mode,
}

impl Emulator {
    pub fn new(cartridge: Cartridge, options: Arc<Options>) -> Self {
        Emulator {
            cartridge,
            options,
            output_buffer: SharedOutputBuffer::new(),
            tick: 0,
            frame: 0,
            mode: Mode::OAMScan,
        }
    }

    fn current_position_in_scanline(&self) -> usize {
        self.tick as usize % TICKS_PER_SCANLINE
    }

    fn current_virtual_scanline(&self) -> usize {
        self.tick as usize / TICKS_PER_SCANLINE
    }

    pub fn write_pixel(&self, x: usize, y: usize, color: Color32) {
        self.output_buffer.write_pixel(x, y, color);
    }

    pub fn clone_output_buffer(&self) -> SharedOutputBuffer {
        self.output_buffer.clone()
    }

    pub fn insert_cartridge(&mut self, cartridge: Cartridge) {
        // TODO
    }

    /// Run the emulator at the GameBoy's native framerate
    pub fn run(&mut self) {
        let start_time = Instant::now();

        loop {
            self.tick = 0;

            let frame_start_nanos = duration_to_nanos(Instant::now().duration_since(start_time));
            if self.options.log_frames {
                let expected_frame_start_nanos = NS_PER_FRAME as u64 * self.frame;
                let frame_start_diff_nanos =
                    frame_start_nanos as i64 - expected_frame_start_nanos as i64;
                println!(
                    "[FRAME] Frame start at {}ns, frame {}, {:.2}% through frame",
                    frame_start_nanos,
                    self.frame,
                    frame_start_diff_nanos as f64 / NS_PER_FRAME * 100.0
                );
            }

            // Run a single frame
            for _ in 0..TICKS_PER_FRAME {
                self.run_tick();
            }

            // Increment frame number
            self.frame += 1;

            // Target time (since start) to run the next frame
            let next_frame_time_nanos = NS_PER_FRAME as u64 * self.frame;

            // Current time (since start)
            let current_time = Instant::now();
            let current_time_nanos = duration_to_nanos(current_time.duration_since(start_time));

            if next_frame_time_nanos <= current_time_nanos {
                if self.options.log_frames {
                    println!(
                        "[FRAME] Missed frame {} by {}ns",
                        self.frame,
                        current_time_nanos - next_frame_time_nanos
                    );
                }

                // We're late for the next frame, so skip sleeping
                continue;
            } else {
                // Calculate how long to sleep until the next frame
                let nanos_to_next_frame = next_frame_time_nanos - current_time_nanos;

                if self.options.log_frames {
                    println!(
                        "[FRAME] Frame end at {}ns, frame {}, {:.2}% of frame budget used",
                        current_time_nanos,
                        self.frame - 1,
                        ((current_time_nanos - frame_start_nanos) as f64 / NS_PER_FRAME) * 100.0
                    );
                }

                thread::sleep(Duration::from_nanos(
                    nanos_to_next_frame - (NS_PER_FRAME as u64 / 20),
                ));
            }
        }
    }

    fn run_tick(&mut self) {
        self.tick += 1;

        self.update_mode_for_current_tick();

        // TODO: Execute instruction

        match self.mode {
            Mode::Draw => {
                // TODO: Write a pixel
            }
            Mode::VBlank | Mode::HBlank => {}
            Mode::OAMScan => {}
        }
    }

    /// Switch modes if needed at the start of a tick
    fn update_mode_for_current_tick(&mut self) {
        let scanline_position = self.current_position_in_scanline();

        if scanline_position == OAM_SCAN_TICKS {
            // Reached end of OAM Scan, start Draw
            self.mode = Mode::Draw;
        } else if scanline_position == 0 {
            // Start of new scanline. Could be OAM if scanline is on screen, otherwise first
            // scanline off the screen starts VBlank
            let current_virtual_scanline = self.current_virtual_scanline();
            if current_virtual_scanline < SCREEN_HEIGHT {
                self.mode = Mode::OAMScan;
            } else if current_virtual_scanline == SCREEN_HEIGHT {
                self.mode = Mode::VBlank;
            }
        }

        // TODO: Switch to HBlank at end of draw period of variable length
    }

    /// Read a byte from the given virtual address.
    ///
    /// May be mapped to a register or may be mapped to cartridge memory via the MBC.
    fn read_address(&self, addr: Address) -> u8 {
        if addr < ROM_END {
            // No support needed yet for reading registers from RAM area
            let mapped_addr = self.cartridge.mbc().map_read_rom_address(addr);
            self.cartridge.rom()[mapped_addr]
        } else if addr < VRAM_END {
            todo!("Read from VRAM")
        } else if addr < EXTERNAL_RAM_END {
            match self.cartridge.mbc().map_read_ram_address(addr) {
                Location::Address(mapped_addr) => self.cartridge.ram()[mapped_addr],
                Location::Register(reg) => self.cartridge.mbc().read_register(reg),
            }
        } else {
            todo!("Unsupported read")
        }
    }

    /// Write a byte to the given virtual address.
    ///
    /// May be mapped to a register or may be mapped to cartridge memory via the MBC.
    fn write_address(&mut self, addr: Address, value: u8) {
        if addr < ROM_END {
            match self.cartridge.mbc().map_write_rom_address(addr) {
                // Writes to physical ROM memory are ignored
                Location::Address(_) => {}
                Location::Register(reg) => self.cartridge.mbc_mut().write_register(reg, value),
            }
        } else if addr < VRAM_END {
            todo!("Write to VRAM")
        } else if addr < EXTERNAL_RAM_END {
            match self.cartridge.mbc().map_write_ram_address(addr) {
                Location::Address(mapped_addr) => self.cartridge.ram_mut()[mapped_addr] = value,
                Location::Register(reg) => self.cartridge.mbc_mut().write_register(reg, value),
            }
        } else {
            todo!("Unsupported write")
        }
    }
}

/// Convert a duration to nanoseconds, assuming it fits in u64.
fn duration_to_nanos(duration: Duration) -> u64 {
    let seconds = duration.as_secs();
    let subsec_nanos = duration.subsec_nanos() as u64;
    seconds * 1_000_000_000 + subsec_nanos
}
