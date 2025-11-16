use std::{
    array,
    collections::VecDeque,
    mem,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
        mpsc::Receiver,
    },
    thread,
    time::{Duration, Instant},
};

use eframe::egui::Color32;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::{
    address_space::{
        Address, ECHO_RAM_END, EXTERNAL_RAM_END, FIRST_WORK_RAM_BANK_END,
        FIRST_WORK_RAM_BANK_START, HRAM_END, HRAM_SIZE, HRAM_START, IE_ADDRESS, IO_REGISTERS_END,
        OAM_END, OAM_SIZE, OAM_START, ROM_END, SECOND_WORK_RAM_BANK_END,
        SECOND_WORK_RAM_BANK_START, SINGLE_VRAM_BANK_SIZE, SINGLE_WORK_RAM_BANK_SIZE,
        UNUSABLE_SPACE_END, VRAM_END, VRAM_START,
    },
    audio::{Apu, AudioOutput, TICKS_PER_SAMPLE, TimedSample},
    cartridge::Cartridge,
    io_registers::IoRegisters,
    machine::Machine,
    mbc::mbc::Location,
    options::Options,
    ppu::{Color, WindowLineCounter, draw_scanline},
    registers::Registers,
    save_file::{NUM_QUICK_SAVE_SLOTS, SAVE_FILE_AUTO_FLUSH_INTERVAL_SECS, SaveFile},
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

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum Button {
    A = 0x01,
    B = 0x02,
    Select = 0x04,
    Start = 0x08,
    Right = 0x10,
    Left = 0x20,
    Up = 0x40,
    Down = 0x80,
}

type ButtonSet = u8;

pub struct SharedInputAdapter {
    commands_rx: Receiver<Command>,
}

pub enum Command {
    /// Send a new set of pressed buttons encoded as a byte
    UpdatePressedButtons(u8),
    /// Save the entire emulator state to disk
    Save,
    /// Save emulator state into the given quick save slot
    QuickSave(usize),
    /// Load a quick save from the given slot
    LoadQuickSave(usize),
    /// Set whether the emulator is in turbo mode
    SetTurboMode(bool),
    /// Increase volume of the emulator
    VolumeUp,
    /// Decrease volume of the emulator
    VolumeDown,
    /// Toggle mute on or off
    ToggleMute,
    /// Toggle the given audio channel on or off
    ToggleAudioChannel(usize),
}

impl SharedInputAdapter {
    pub fn new(commands_rx: Receiver<Command>) -> Self {
        Self { commands_rx }
    }
}

/// The default grayscale color palette.
const SCREEN_COLOR_PALETTE_GRAYSCALE: [Color32; 4] = [
    Color32::from_rgb(0xFF, 0xFF, 0xFF),
    Color32::from_rgb(0xAA, 0xAA, 0xAA),
    Color32::from_rgb(0x55, 0x55, 0x55),
    Color32::from_rgb(0x00, 0x00, 0x00),
];

/// A green color palette for the original GameBoy screen.
/// TODO: Configure screen color palette via options.
#[allow(unused)]
const SCREEN_COLOR_PALETTE_GREEN: [Color32; 4] = [
    Color32::from_rgb(0x9B, 0xBC, 0x0F),
    Color32::from_rgb(0x8B, 0xAC, 0x0F),
    Color32::from_rgb(0x30, 0x62, 0x30),
    Color32::from_rgb(0x0F, 0x38, 0x0F),
];

pub type Register = u8;

/// Refresh rate of the GameBoy screen in Hz
pub const REFRESH_RATE: f64 = 59.7;

/// Total number of scanlines including VBlank period. Larger than the height of the screen.
const NUM_VIRTUAL_SCANLINES: usize = 154;

pub const TICKS_PER_FRAME: usize = 70224;

const TICKS_PER_SCANLINE: usize = TICKS_PER_FRAME / NUM_VIRTUAL_SCANLINES;

/// Number of ticks in OAM Scan mode at the beginning of each scanline
const OAM_SCAN_TICKS: usize = 80;

/// Number of ticks in Draw mode. In reality this is variable, but we choose the minimum time here.
const DRAW_TICKS: usize = 172;

const HBLANK_TICKS: usize = TICKS_PER_SCANLINE - OAM_SCAN_TICKS - DRAW_TICKS;

/// Total number of ticks to complete an OAM DMA transfer
const OAM_DMA_TRANSFER_TICKS: usize = 640;

/// Size of a single block transferred in a VRAM DMA transfer
const VRAM_DMA_TRANSFER_BLOCK_SIZE: u16 = 16;

/// Number of ticks to transfer a single 16-byte block in a VRAM DMA transfer
const VRAM_DMA_TRANSFER_TICKS_PER_BLOCK: usize = 32;

/// Number of ticks to halt after executing a speed switch
const SPEED_SWITCH_TICKS: usize = 0x20000;

/// How much faster the emulator tries to run in turbo mode
const TURBO_MULTIPLIER: u64 = 10;

/// Nanoseconds in real time per frame in regular mode
const NS_PER_FRAME: f64 = 1_000_000_000.0f64 / REFRESH_RATE;

/// Nanoseconds in real time per frame in turbo mode
const NS_PER_MICROFRAME: f64 = NS_PER_FRAME / (TURBO_MULTIPLIER as f64);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Mode {
    /// Mode 0: Move to the next scanline
    HBlank,
    /// Mode 1: Move back to start of screen
    VBlank,
    /// Mode 2: Search OAM for sprites in current scanline
    OamScan,
    /// Mode 3: Draw pixels to screen
    Draw,
}

impl Mode {
    pub fn byte_value(&self) -> u8 {
        match self {
            Mode::HBlank => 0,
            Mode::VBlank => 1,
            Mode::OamScan => 2,
            Mode::Draw => 3,
        }
    }
}

pub enum Interrupt {
    VBlank,
    LcdStat,
    Timer,
    Serial,
    Joypad,
}

impl Interrupt {
    fn flag_bit(&self) -> u8 {
        match self {
            Interrupt::VBlank => 0x01,
            Interrupt::LcdStat => 0x02,
            Interrupt::Timer => 0x04,
            Interrupt::Serial => 0x08,
            Interrupt::Joypad => 0x10,
        }
    }

    pub fn handler_address(&self) -> Address {
        match self {
            Interrupt::VBlank => 0x40,
            Interrupt::LcdStat => 0x48,
            Interrupt::Timer => 0x50,
            Interrupt::Serial => 0x58,
            Interrupt::Joypad => 0x60,
        }
    }

    /// Return the highest priority interrupt in the bit string from IE or IF.
    fn for_bits(interrupt_bits: u8) -> Self {
        if (interrupt_bits & 0x01) != 0 {
            Interrupt::VBlank
        } else if (interrupt_bits & 0x02) != 0 {
            Interrupt::LcdStat
        } else if (interrupt_bits & 0x04) != 0 {
            Interrupt::Timer
        } else if (interrupt_bits & 0x08) != 0 {
            Interrupt::Serial
        } else if (interrupt_bits & 0x10) != 0 {
            Interrupt::Joypad
        } else {
            unreachable!()
        }
    }
}

/// The `ei` instruction enables interrupts after the next instruction completes. Use a small state
/// machine to simulate this.
#[derive(Serialize, Deserialize)]
enum PendingEnableInterrupt {
    None,
    AfterNextInstruction,
    /// Set to repeat if two `ei` instructions are executed back-to-back.
    AfterCurrentInstruction {
        repeat: bool,
    },
}

#[derive(Serialize, Deserialize)]
struct OamDmaTransfer {
    /// The source address which data is copied from into OAM
    source_address: Address,
    /// The number of ticks until this transfer is complete
    ticks_remaining: usize,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum VramDmaTransferKind {
    /// Program execution is halted until DMA transfer completes
    GeneralPurpose,
    /// Transfer occurs during HBlank periods
    HBlank,
}

/// An HBlank DMA transfer in progress
#[derive(Serialize, Deserialize)]
pub struct VramDmaTransfer {
    source: Address,
    dest: Address,
    remaining_ticks_in_current_hblank: Option<usize>,
    num_blocks_left: u8,
    total_num_blocks: u8,
}

const TAC_MASK_16_TICKS: u16 = 0x0008;
const TAC_MASK_64_TICKS: u16 = 0x0020;
const TAC_MASK_256_TICKS: u16 = 0x0080;
const TAC_MASK_1024_TICKS: u16 = 0x0200;

const IE_INIT: Register = 0x00;

/// Value returned when reading from VRAM (or CGB palettes) during draw mode
pub const VRAM_READ_FAILED_VALUE: u8 = 0xFF;

pub type CgbPaletteData = [u8; 64];
type StoredCgbPalette = serde_big_array::Array<u8, 64>;

#[derive(Serialize, Deserialize)]
pub struct Emulator {
    /// Cartridge inserted
    cartridge: Cartridge,

    /// Options for the emulator
    #[serde(skip)]
    options: Arc<Options>,

    /// Input adapter for reading button presses from the GUI thread
    #[serde(skip)]
    input_adapter: Option<SharedInputAdapter>,

    /// Output buffer for the screen which is shared with the GUI thread
    #[serde(skip)]
    output_buffer: Option<SharedOutputBuffer>,

    /// Sender for audio samples, batched by frame
    #[serde(skip)]
    audio_output: Option<Box<dyn AudioOutput>>,

    /// The save file for this ROM, if any
    #[serde(skip)]
    save_file: Option<Box<SaveFile>>,

    /// The path to write the save file to disk, if any
    #[serde(skip)]
    save_file_path: Option<String>,

    /// The machine type being emulated (DMG or CGB)
    machine: Machine,

    /// Current tick (T-cycle) within a frame
    tick: u32,

    /// Current turbo frame number. The microframe number divided by the turbo multiplier is the
    /// frame number in regular mode.
    #[serde(skip)]
    microframe: u64,

    /// Current (virtual) scanline number
    scanline: u8,

    /// Current mode
    mode: Mode,

    /// Whether the emulator is currently in CGB mode
    in_cgb_mode: bool,

    /// VRAM region, including all banks
    #[serde(with = "serde_bytes")]
    vram: Vec<u8>,

    /// OAM region (Object Attribute Memory)
    #[serde(with = "serde_bytes")]
    oam: Vec<u8>,

    /// HRAM region (High RAM)
    #[serde(with = "serde_bytes")]
    hram: Vec<u8>,

    /// Work RAM region, including all banks
    #[serde(with = "serde_bytes")]
    work_ram: Vec<u8>,

    /// Register file
    regs: Registers,

    /// IO register file
    io_regs: IoRegisters,

    /// Interrupt Enable register (0xFFFF) (IE)
    ie: Register,

    /// Audio Processing Unit
    apu: Apu,

    /// Background color palettes (CGB only)
    cgb_background_palettes: Box<StoredCgbPalette>,

    /// Object color palettes (CGB only)
    cgb_object_palettes: Box<StoredCgbPalette>,

    /// Number of ticks remaining until the next instruction is executed
    ticks_to_next_instruction: usize,

    /// State machine for `ei` instructions to enable interrupts after the next instruction
    pending_enable_interrupts: PendingEnableInterrupt,

    /// The current OAM DMA transfer, if one is in progress
    current_oam_dma_transfer: Option<OamDmaTransfer>,

    /// The current HBlank VRAM DMA transfer, if one is in progress
    current_hblank_vram_dma_transfer: Option<VramDmaTransfer>,

    /// The number of ticks remaining in a general purpose VRAM DMA transfer, if one is in progress
    current_general_purpose_vram_dma_transfer: Option<usize>,

    /// The number of ticks remaining in the current CPU halt after a speed switch was executed
    current_speed_switch: Option<usize>,

    /// Whether the CPU is currently halted
    is_cpu_halted: bool,

    /// Whether the CPU is currently stopped due to a VRAM DMA transfer
    is_cpu_stopped_for_vram_dma: bool,

    /// Internal line number counter used for rendering the window
    window_line_counter: WindowLineCounter,

    /// Set of currently pressed buttons, exposed via the joypad register
    pressed_buttons: ButtonSet,

    /// Internal clock divider register - 16 bits but only the upper 8 bits are exposed via DIV.
    full_divider_register: u16,

    /// Current divider register mask set by TAC register, increment TIMA when the single bit in
    /// this mask detects a falling edge (changed from 1 to 0).
    tac_mask: u16,

    /// Whether the timer (TIMA) can be incremented
    is_timer_enabled: bool,

    /// Whether the emulator is currently in turbo mode
    #[serde(skip)]
    in_turbo_mode: bool,

    /// Whether the emulator is currently booting (running the boot ROM)
    is_booting: bool,

    /// Whether the emulator is currently in double speed mode
    is_double_speed: bool,

    /// Queue of audio samples built in the current frame
    audio_sample_queue: VecDeque<TimedSample>,
}

pub struct EmulatorBuilder {
    emulator: Emulator,
}

impl EmulatorBuilder {
    fn new(emulator: Emulator) -> Self {
        EmulatorBuilder { emulator }
    }

    pub fn new_cartridge(cartridge: Cartridge, machine: Machine) -> Self {
        let mut emulator = Emulator::initial_state(cartridge, machine);
        emulator.save_file = Some(Box::new(SaveFile::new(emulator.cartridge())));

        Self::new(emulator)
    }

    pub fn from_saved_cartidge(save_file: Box<SaveFile>, machine: Machine) -> Self {
        let cartridge = rmp_serde::from_slice(&save_file.cartridge).unwrap();

        let mut emulator = Emulator::initial_state(cartridge, machine);
        emulator.save_file = Some(save_file);

        Self::new(emulator)
    }

    pub fn from_quick_save_bytes(save_file: Box<SaveFile>, serialized_bytes: &[u8]) -> Self {
        let mut emulator: Emulator = rmp_serde::from_slice(serialized_bytes).unwrap();
        emulator.save_file = Some(save_file);

        Self::new(emulator)
    }

    pub fn with_options(mut self, options: Arc<Options>) -> Self {
        self.emulator.options = options;
        self
    }

    pub fn with_save_file_path(mut self, save_file_path: String) -> Self {
        self.emulator.save_file_path = Some(save_file_path);
        self
    }

    pub fn with_input_adapter(mut self, input_adapter: SharedInputAdapter) -> Self {
        self.emulator.input_adapter = Some(input_adapter);
        self
    }

    pub fn with_output_buffer(mut self, output_buffer: SharedOutputBuffer) -> Self {
        self.emulator.output_buffer = Some(output_buffer);
        self
    }

    pub fn with_audio_output(mut self, audio_output: Box<dyn AudioOutput>) -> Self {
        self.emulator.audio_output = Some(audio_output);
        self
    }

    pub fn build(self) -> Emulator {
        self.emulator
    }
}

impl Emulator {
    /// The initial state of the emulator for a given cartridge and machine type.
    ///
    /// Initialized to the standard state after the BIOS has run and the cartridge entry point code
    /// is ready to execute.
    fn initial_state(cartridge: Cartridge, machine: Machine) -> Self {
        Emulator {
            cartridge,
            options: Arc::new(Options::default()),
            input_adapter: None,
            output_buffer: None,
            audio_output: None,
            save_file: None,
            save_file_path: None,
            machine,
            tick: 0,
            microframe: 0,
            scanline: 0,
            mode: Mode::OamScan,
            in_cgb_mode: false,
            vram: vec![0; machine.vram_size()],
            oam: vec![0; OAM_SIZE],
            hram: vec![0; HRAM_SIZE],
            work_ram: vec![0; machine.wram_size()],
            // CGB background palettes initialized to all white
            cgb_background_palettes: Box::new(serde_big_array::Array([0xFF; 64])),
            // CGB object palettes only requires that first byte is 0x00, rest are uninitialized
            cgb_object_palettes: Box::new(serde_big_array::Array([0x00; 64])),
            regs: Registers::init_for_machine(machine),
            io_regs: IoRegisters::init_for_machine(machine),
            apu: Apu::new(),
            ie: IE_INIT,
            ticks_to_next_instruction: 0,
            pending_enable_interrupts: PendingEnableInterrupt::None,
            current_oam_dma_transfer: None,
            current_hblank_vram_dma_transfer: None,
            current_general_purpose_vram_dma_transfer: None,
            current_speed_switch: None,
            is_cpu_halted: false,
            is_cpu_stopped_for_vram_dma: false,
            window_line_counter: WindowLineCounter::new(),
            pressed_buttons: 0,
            full_divider_register: 0,
            tac_mask: TAC_MASK_1024_TICKS,
            is_timer_enabled: false,
            in_turbo_mode: false,
            is_booting: true,
            is_double_speed: false,
            audio_sample_queue: VecDeque::new(),
        }
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    pub fn scanline(&self) -> u8 {
        self.scanline
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn in_cgb_mode(&self) -> bool {
        self.in_cgb_mode
    }

    pub fn set_in_cgb_mode(&mut self, in_cgb_mode: bool) {
        self.in_cgb_mode = in_cgb_mode;
    }

    pub fn is_cgb_machine(&self) -> bool {
        matches!(self.machine, Machine::Cgb)
    }

    pub fn in_test_mode(&self) -> bool {
        self.options.in_test_mode
    }

    pub fn vram(&self) -> &[u8] {
        &self.vram
    }

    pub fn oam(&self) -> &[u8] {
        &self.oam
    }

    pub fn regs(&self) -> &Registers {
        &self.regs
    }

    pub fn regs_mut(&mut self) -> &mut Registers {
        &mut self.regs
    }

    pub fn io_regs(&self) -> &IoRegisters {
        &self.io_regs
    }

    pub fn io_regs_mut(&mut self) -> &mut IoRegisters {
        &mut self.io_regs
    }

    pub fn apu(&self) -> &Apu {
        &self.apu
    }

    pub fn apu_mut(&mut self) -> &mut Apu {
        &mut self.apu
    }

    pub fn cgb_background_palettes(&self) -> &CgbPaletteData {
        &self.cgb_background_palettes
    }

    pub fn cgb_background_palettes_mut(&mut self) -> &mut CgbPaletteData {
        &mut self.cgb_background_palettes
    }

    pub fn cgb_object_palettes(&self) -> &CgbPaletteData {
        &self.cgb_object_palettes
    }

    pub fn cgb_object_palettes_mut(&mut self) -> &mut CgbPaletteData {
        &mut self.cgb_object_palettes
    }

    pub fn halt_cpu(&mut self) {
        self.is_cpu_halted = true;
    }

    pub fn resume_halted_cpu(&mut self) {
        self.is_cpu_halted = false;

        // Reset the speed switch countdown until the current halt is cleared
        self.current_speed_switch = None;
    }

    pub fn window_line_counter_mut(&mut self) -> &mut WindowLineCounter {
        &mut self.window_line_counter
    }

    pub fn pressed_buttons(&self) -> ButtonSet {
        self.pressed_buttons
    }

    pub fn full_divider_register(&self) -> u16 {
        self.full_divider_register
    }

    pub fn reset_divider_register(&mut self) {
        self.full_divider_register = 0;
    }

    pub fn is_timer_enabled(&self) -> bool {
        self.is_timer_enabled
    }

    pub fn set_timer_enabled(&mut self, enabled: bool) {
        self.is_timer_enabled = enabled;
    }

    pub fn is_booting(&self) -> bool {
        self.is_booting
    }

    pub fn set_is_booting(&mut self, is_booting: bool) {
        self.is_booting = is_booting;
    }

    pub fn is_double_speed(&self) -> bool {
        self.is_double_speed
    }

    pub fn set_is_double_speed(&mut self, is_double_speed: bool) {
        self.is_double_speed = is_double_speed;
    }

    pub fn tick_number(&self) -> u32 {
        self.tick
    }

    /// Map from the divider register mask to the corresponding bits of the TAC register
    pub fn tac_bits(&self) -> u8 {
        match self.tac_mask {
            TAC_MASK_1024_TICKS => 0b00,
            TAC_MASK_16_TICKS => 0b01,
            TAC_MASK_64_TICKS => 0b10,
            TAC_MASK_256_TICKS => 0b11,
            _ => unreachable!(),
        }
    }

    /// Map from bits of the TAC register to the corresponding divider register mask
    pub fn set_tac_bits(&mut self, tac_bits: u8) {
        let tac_mask = match tac_bits & 0x3 {
            0b00 => TAC_MASK_1024_TICKS,
            0b01 => TAC_MASK_16_TICKS,
            0b10 => TAC_MASK_64_TICKS,
            0b11 => TAC_MASK_256_TICKS,
            _ => unreachable!(),
        };

        self.tac_mask = tac_mask;
    }

    /// Whether we can currently access VRAM and the CGB palette data
    pub fn can_access_vram(&self) -> bool {
        self.mode != Mode::Draw || !self.is_lcdc_lcd_enabled()
    }

    pub fn clone_output_buffer(&self) -> Option<SharedOutputBuffer> {
        self.output_buffer.clone()
    }

    fn ns_per_frame(&self) -> f64 {
        if self.in_turbo_mode {
            NS_PER_MICROFRAME
        } else {
            NS_PER_FRAME
        }
    }

    fn microframes_per_frame(&self) -> u64 {
        if self.in_turbo_mode {
            1
        } else {
            TURBO_MULTIPLIER
        }
    }

    /// The expected start time in nanoseconds for the current frame (or microframe)
    fn expected_frame_start_nanos(&self) -> u64 {
        ((self.microframe as f64) * NS_PER_MICROFRAME) as u64
    }

    fn format_frame_number(&self, microframe: u64) -> String {
        if self.in_turbo_mode {
            format!("{:.1}", (microframe as f64) / (TURBO_MULTIPLIER as f64))
        } else {
            format!("{}", microframe / TURBO_MULTIPLIER)
        }
    }

    /// Run the emulator at the GameBoy's native framerate
    pub fn run(&mut self) {
        self.emulate_boot_sequence();

        let start_time = Instant::now();
        let mut last_save_file_flush_time = Instant::now();

        loop {
            let frame_start_nanos = duration_to_nanos(Instant::now().duration_since(start_time));
            if self.options.log_frames {
                let expected_frame_start_nanos = self.expected_frame_start_nanos();
                let frame_start_diff_nanos =
                    frame_start_nanos as i64 - expected_frame_start_nanos as i64;
                println!(
                    "[FRAME] Frame start at {}ns, frame {}, {:.2}% through frame",
                    frame_start_nanos,
                    self.format_frame_number(self.microframe),
                    frame_start_diff_nanos as f64 / self.ns_per_frame() * 100.0
                );
            }

            // Run a single frame
            self.run_frame();

            // Push a single audio frame to the audio output, if any
            if let Some(audio_output) = &mut self.audio_output {
                let mut audio_frame = VecDeque::new();
                mem::swap(&mut audio_frame, &mut self.audio_sample_queue);
                audio_output.send_frame(audio_frame);
            }

            // Increment frame number (backed by microframes)
            self.microframe += self.microframes_per_frame();

            // Target time (since start) to run the next frame
            let mut next_frame_time_nanos = self.expected_frame_start_nanos();

            // Current time (since start)
            let current_time = Instant::now();
            let current_time_nanos = duration_to_nanos(current_time.duration_since(start_time));

            // Flush the save file to disk at regular intervals
            if current_time
                .duration_since(last_save_file_flush_time)
                .as_secs()
                >= SAVE_FILE_AUTO_FLUSH_INTERVAL_SECS
            {
                last_save_file_flush_time = Instant::now();
                self.save_cartridge_state_to_disk();
            }

            if self.options.log_frames {
                println!(
                    "[FRAME] Frame end at {}ns, frame {}, {:.2}% of frame budget used",
                    current_time_nanos,
                    self.format_frame_number(self.microframe - self.microframes_per_frame()),
                    ((current_time_nanos - frame_start_nanos) as f64 / self.ns_per_frame()) * 100.0
                );
            }

            // Schedule the next frame and sleep until then
            if next_frame_time_nanos > current_time_nanos {
                // Calculate how long to sleep until the next frame
                let nanos_to_next_frame = next_frame_time_nanos - current_time_nanos;

                thread::sleep(Duration::from_nanos(
                    nanos_to_next_frame.saturating_sub(self.ns_per_frame() as u64 / 20),
                ));

                continue;
            }

            // Skip frames whose expected start time has already passed
            while next_frame_time_nanos <= current_time_nanos {
                if self.options.log_frames {
                    println!(
                        "[FRAME] Missed frame {} by {}ns",
                        self.format_frame_number(self.microframe),
                        current_time_nanos - next_frame_time_nanos
                    );
                }

                self.microframe += self.microframes_per_frame();
                next_frame_time_nanos = self.expected_frame_start_nanos();
            }

            // Continue directly to the next frame, starting it early since a frame was skipped
        }
    }

    fn run_frame(&mut self) {
        self.tick = 0;

        for i in 0..(NUM_VIRTUAL_SCANLINES as u8) {
            self.run_scanline(i);
        }
    }

    fn run_scanline(&mut self, scanline: u8) {
        self.scanline = scanline;

        // Resust interrupt for LYC=LY if necessary
        if self.is_stat_lyc_interrupt_enabled() && (self.scanline == self.lyc()) {
            self.request_interrupt(Interrupt::LcdStat);
        }

        if scanline < SCREEN_HEIGHT as u8 {
            // Each scanline starts in OAM scan mode
            self.set_mode(Mode::OamScan);
            for _ in 0..OAM_SCAN_TICKS {
                self.run_tick();
            }

            // Followed by a draw period. We simplify by making this a fixed length and drawing the
            // entire scanline at once, at the start of the draw period.
            self.set_mode(Mode::Draw);

            draw_scanline(self, scanline);

            for _ in 0..DRAW_TICKS {
                self.run_tick();
            }

            // Finally enter HBlank for the rest of the scanline
            self.enter_hblank();
            for _ in 0..HBLANK_TICKS {
                self.run_tick();
            }
        } else {
            // Enter VBlank at the start of the first scanline after the screen
            if scanline == SCREEN_HEIGHT as u8 {
                self.enter_vblank();
            }

            // VBlank simply ticks along with nothing drawn
            for _ in 0..TICKS_PER_SCANLINE {
                self.run_tick();
            }
        }
    }

    fn enter_vblank(&mut self) {
        self.set_mode(Mode::VBlank);
        self.window_line_counter.reset();
    }

    fn enter_hblank(&mut self) {
        self.set_mode(Mode::HBlank);

        if self.current_hblank_vram_dma_transfer.is_some() {
            self.start_hblank_vram_dma_transfer_block();
        }
    }

    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;

        match mode {
            Mode::HBlank => {
                if self.is_stat_hblank_interrupt_enabled() {
                    self.request_interrupt(Interrupt::LcdStat);
                }
            }
            Mode::VBlank => {
                self.request_interrupt(Interrupt::VBlank);

                if self.is_stat_vblank_interrupt_enabled() {
                    self.request_interrupt(Interrupt::LcdStat);
                }
            }
            Mode::OamScan => {
                if self.is_stat_oam_scan_interrupt_enabled() {
                    self.request_interrupt(Interrupt::LcdStat);
                }
            }
            Mode::Draw => {}
        }
    }

    pub fn request_interrupt(&mut self, interrupt: Interrupt) {
        let current_if = self.if_reg();
        self.write_if_reg(current_if | interrupt.flag_bit());
    }

    pub fn schedule_next_instruction(&mut self, ticks: usize) {
        self.ticks_to_next_instruction = ticks;
    }

    fn run_tick(&mut self) {
        self.handle_inputs();
        self.increment_timers();

        let old_tick_number = self.tick;
        self.apu_mut().advance_period_timers(old_tick_number);

        // Ready for next instruction. Either execute the next instruction or an interrupt handler.
        'handled: {
            if self.ticks_to_next_instruction == 0 {
                let interrupt_bits = self.interrupt_bits();
                if interrupt_bits != 0 {
                    // A pending interrupts resumes a halted CPU, even if IME is disabled and
                    // interrupt won't actually be handled.
                    self.resume_halted_cpu();

                    if self.regs().interrupts_enabled() {
                        self.handle_interrupt(Interrupt::for_bits(interrupt_bits));
                        break 'handled;
                    }
                }

                if !self.is_cpu_halted && !self.is_cpu_stopped_for_vram_dma {
                    self.execute_instruction();
                    break 'handled;
                }
            }
        }

        // Sample audio if necessary
        if self.tick % TICKS_PER_SAMPLE as u32 == 0 {
            self.push_next_sample();
        }

        self.tick += 1;

        // CPU runs twice as fast in double speed mode
        if self.is_double_speed() {
            self.ticks_to_next_instruction = self.ticks_to_next_instruction.saturating_sub(2);
        } else {
            self.ticks_to_next_instruction = self.ticks_to_next_instruction.saturating_sub(1);
        }

        // Advance states at the end of the tick
        self.advance_pending_enable_interrupts_state();
        self.advance_oam_dma_transfer_state();
        self.advance_general_purpose_vram_dma_transfer_state();
        self.advance_hblank_vram_dma_transfer_state();
        self.advance_speed_switch_state();
    }

    fn handle_inputs(&mut self) {
        if self.input_adapter.is_none() {
            return;
        }

        while let Ok(command) = self.input_adapter.as_ref().unwrap().commands_rx.try_recv() {
            match command {
                Command::UpdatePressedButtons(new_pressed_buttons) => {
                    self.handle_update_pressed_buttons(new_pressed_buttons)
                }
                Command::Save => self.save_cartridge_state_to_disk(),
                Command::QuickSave(slot) => self.quick_save(slot),
                Command::LoadQuickSave(slot) => self.load_quick_save(slot),
                Command::SetTurboMode(in_turbo_mode) => self.in_turbo_mode = in_turbo_mode,
                Command::VolumeUp => self.apu_mut().increase_system_volume(),
                Command::VolumeDown => self.apu_mut().decrease_system_volume(),
                Command::ToggleMute => self.apu_mut().toggle_muted(),
                Command::ToggleAudioChannel(channel) => self.apu_mut().toggle_channel(channel),
            }
        }
    }

    fn quick_save(&mut self, slot: usize) {
        if slot >= NUM_QUICK_SAVE_SLOTS || self.save_file.is_none() {
            return;
        }

        let emulator_bytes = rmp_serde::to_vec(self).unwrap();

        let save_file = self.save_file.as_mut().unwrap();
        save_file.quick_saves[slot] = Some(ByteBuf::from(emulator_bytes));

        if let Some(save_file_path) = &mut self.save_file_path {
            save_file.flush_to_disk(save_file_path);
        }
    }

    fn load_quick_save(&mut self, slot: usize) {
        if slot >= NUM_QUICK_SAVE_SLOTS
            || self.save_file.is_none()
            || self.save_file.as_ref().unwrap().quick_saves[slot].is_none()
        {
            return;
        }

        // Deserialize emulator state
        let save_file = self.save_file.take().unwrap();
        let serialized_bytes = save_file.quick_saves[slot].as_ref().unwrap().to_vec();

        // Some state was not included in serialization and must be preserved
        let microframe = self.microframe;

        let mut emulator_builder =
            EmulatorBuilder::from_quick_save_bytes(save_file, &serialized_bytes)
                .with_options(self.options.clone());

        if let Some(save_file_path) = self.save_file_path.take() {
            emulator_builder = emulator_builder.with_save_file_path(save_file_path);
        }

        if let Some(input_adapter) = self.input_adapter.take() {
            emulator_builder = emulator_builder.with_input_adapter(input_adapter);
        }

        if let Some(output_buffer) = self.output_buffer.take() {
            emulator_builder = emulator_builder.with_output_buffer(output_buffer);
        }

        if let Some(audio_output) = self.audio_output.take() {
            emulator_builder = emulator_builder.with_audio_output(audio_output);
        }

        *self = emulator_builder.build();

        // Restore state excluded from quick save
        self.microframe = microframe;
    }

    fn handle_update_pressed_buttons(&mut self, new_pressed_buttons: u8) {
        if self.pressed_buttons == new_pressed_buttons {
            return;
        }

        // Convert to the joypad register format. Only write the buttons if the appropriate
        // selection bits are set in the joypad register.
        let old_joypad_reg = self.joypad_reg();
        let select_special_buttons = (old_joypad_reg & 0x20) == 0;
        let select_directional_buttons = (old_joypad_reg & 0x10) == 0;

        let new_joypad_reg = Self::buttons_to_joypad_reg(
            new_pressed_buttons,
            select_special_buttons,
            select_directional_buttons,
        );

        // Request a joypad interrupt if any of the lower 4 bits changed from 1 to 0
        let needs_interrupt = (old_joypad_reg & !new_joypad_reg & 0x0F) != 0;
        if needs_interrupt {
            self.request_interrupt(Interrupt::Joypad);
        }

        // Update state to new pressed buttons
        self.pressed_buttons = new_pressed_buttons;
        self.write_joypad_reg(new_joypad_reg);
    }

    pub fn buttons_to_joypad_reg(
        buttons: u8,
        select_special: bool,
        select_directional: bool,
    ) -> Register {
        let mut result = 0xFF;

        result &= if select_special { !0x20 } else { 0xFF };
        result &= if select_directional { !0x10 } else { 0xFF };

        if select_special {
            if (buttons & (Button::Start as u8)) != 0 {
                result &= !0x08;
            }
            if (buttons & (Button::Select as u8)) != 0 {
                result &= !0x04;
            }
            if (buttons & (Button::B as u8)) != 0 {
                result &= !0x02;
            }
            if (buttons & (Button::A as u8)) != 0 {
                result &= !0x01;
            }
        }

        if select_directional {
            if (buttons & (Button::Down as u8)) != 0 {
                result &= !0x08;
            }
            if (buttons & (Button::Up as u8)) != 0 {
                result &= !0x04;
            }
            if (buttons & (Button::Left as u8)) != 0 {
                result &= !0x02;
            }
            if (buttons & (Button::Right as u8)) != 0 {
                result &= !0x01;
            }
        }

        result
    }

    fn save_cartridge_state_to_disk(&mut self) {
        if let (Some(save_file), Some(save_file_path)) = (&mut self.save_file, &self.save_file_path)
        {
            save_file.update_cartridge_state(&self.cartridge);
            save_file.flush_to_disk(save_file_path);
        }
    }

    fn map_5_bit_color_to_8_bit(color: u8) -> u8 {
        // Copy upper 3 bits to lower bits to most regularly distribute the color range
        (color << 3) | (color >> 2)
    }

    pub fn write_color(&self, x: u8, y: u8, color: Color) {
        if let Some(output_buffer) = &self.output_buffer {
            let color32 = match color {
                // Look up 2-bit color in screen palette
                Color::Dmg(color) => SCREEN_COLOR_PALETTE_GRAYSCALE[color as usize],
                // Convert from 5-bit RGB to 8-bit RGB by shifting
                Color::Cgb(color) => {
                    let red = color.red() as u8;
                    let green = color.green() as u8;
                    let blue = color.blue() as u8;

                    // Copy upper 3 bits to lower bits to most regularly distribute the color range
                    Color32::from_rgb(
                        Self::map_5_bit_color_to_8_bit(red),
                        Self::map_5_bit_color_to_8_bit(green),
                        Self::map_5_bit_color_to_8_bit(blue),
                    )
                }
            };

            output_buffer.write_pixel(x as usize, y as usize, color32);
        }
    }

    /// Read a byte from the given virtual address.
    ///
    /// May be mapped to a register or may be mapped to cartridge memory via the MBC.
    pub fn read_address(&self, addr: Address) -> u8 {
        if addr < ROM_END {
            // No support needed yet for reading registers from RAM area
            let mapped_addr = self.cartridge.mbc().map_read_rom_address(addr);
            self.cartridge.rom()[mapped_addr]
        } else if addr < VRAM_END {
            let physical_addr = self.physical_vram_bank_address(addr);
            self.vram[physical_addr]
        } else if addr < EXTERNAL_RAM_END {
            match self.cartridge.mbc().map_read_ram_address(addr) {
                Location::Address(mapped_addr) => self.cartridge.ram()[mapped_addr],
                Location::Register(reg) => self.cartridge.mbc().read_register(reg),
            }
        } else if addr < FIRST_WORK_RAM_BANK_END {
            let physical_addr = self.physical_first_work_ram_bank_address(addr);
            self.work_ram[physical_addr]
        } else if addr < SECOND_WORK_RAM_BANK_END {
            let physical_addr = self.physical_second_work_ram_bank_address(addr);
            self.work_ram[physical_addr]
        } else if addr < ECHO_RAM_END {
            panic!("Attempted to read from Echo RAM at address {:04X}", addr);
        } else if addr < OAM_END {
            let physical_addr = self.physical_oam_address(addr);
            self.oam[physical_addr]
        } else if addr < UNUSABLE_SPACE_END {
            // Unusable memory area returns a value depending on the model
            0xFF
        } else if addr < IO_REGISTERS_END {
            self.read_io_register(addr)
        } else if addr < HRAM_END {
            let physical_addr = self.physical_hram_address(addr);
            self.hram[physical_addr]
        } else if addr == IE_ADDRESS {
            self.ie
        } else {
            unreachable!()
        }
    }

    /// Write a byte to the given virtual address.
    ///
    /// May be mapped to a register or may be mapped to cartridge memory via the MBC.
    pub fn write_address(&mut self, addr: Address, value: u8) {
        if addr < ROM_END {
            match self.cartridge.mbc().map_write_rom_address(addr) {
                // Writes to physical ROM memory are ignored
                Location::Address(_) => {}
                Location::Register(reg) => self.cartridge.mbc_mut().write_register(reg, value),
            }
        } else if addr < VRAM_END {
            let physical_addr = self.physical_vram_bank_address(addr);
            self.vram[physical_addr] = value;
        } else if addr < EXTERNAL_RAM_END {
            match self.cartridge.mbc().map_write_ram_address(addr) {
                Location::Address(mapped_addr) => self.cartridge.ram_mut()[mapped_addr] = value,
                Location::Register(reg) => self.cartridge.mbc_mut().write_register(reg, value),
            }
        } else if addr < FIRST_WORK_RAM_BANK_END {
            let physical_addr = self.physical_first_work_ram_bank_address(addr);
            self.work_ram[physical_addr] = value;
        } else if addr < SECOND_WORK_RAM_BANK_END {
            let physical_addr = self.physical_second_work_ram_bank_address(addr);
            self.work_ram[physical_addr] = value;
        } else if addr < ECHO_RAM_END {
            panic!("Attempted to write to Echo RAM at address {:04X}", addr);
        } else if addr < OAM_END {
            let physical_addr = self.physical_oam_address(addr);
            self.oam[physical_addr] = value;
        } else if addr < UNUSABLE_SPACE_END {
            // Unusable memory area, writes are ignored
        } else if addr < IO_REGISTERS_END {
            self.write_io_register(addr, value)
        } else if addr < HRAM_END {
            let physical_addr = self.physical_hram_address(addr);
            self.hram[physical_addr] = value;
        } else if addr == IE_ADDRESS {
            self.ie = value;
        } else {
            unreachable!()
        }
    }

    /// Map from a virtual address in VRAM to a physical address in the VRAM array for the given
    /// bank.
    pub fn map_vram_address_in_bank(addr: Address, bank_num: usize) -> usize {
        (addr - VRAM_START) as usize + bank_num * SINGLE_VRAM_BANK_SIZE
    }

    /// Map from a virtual address in VRAM to a physical address in the VRAM array for the currently
    /// selected bank in VBK.
    fn physical_vram_bank_address(&self, addr: Address) -> usize {
        let bank_num = if self.in_cgb_mode() {
            (self.vbk() & 0x01) as usize
        } else {
            0
        };

        Self::map_vram_address_in_bank(addr, bank_num)
    }

    fn physical_first_work_ram_bank_address(&self, addr: Address) -> usize {
        (addr - FIRST_WORK_RAM_BANK_START) as usize
    }

    fn second_wram_bank_num(&self) -> usize {
        if self.in_cgb_mode() {
            // Cannot access bank 0, instead return bank 1
            (self.wbk() & 0x7).max(1) as usize
        } else {
            1
        }
    }

    fn physical_second_work_ram_bank_address(&self, addr: Address) -> usize {
        (addr - SECOND_WORK_RAM_BANK_START) as usize
            + SINGLE_WORK_RAM_BANK_SIZE * self.second_wram_bank_num()
    }

    fn physical_oam_address(&self, addr: Address) -> usize {
        (addr - OAM_START) as usize
    }

    fn physical_hram_address(&self, addr: Address) -> usize {
        (addr - HRAM_START) as usize
    }

    pub fn add_pending_enable_interrupts(&mut self) {
        match self.pending_enable_interrupts {
            // `ei` is called without any pending requests
            PendingEnableInterrupt::None => {
                self.pending_enable_interrupts = PendingEnableInterrupt::AfterNextInstruction;
            }
            // `ei` is called while waiting for the previous `ei` to take effect, mark repeating so
            // that we stay in this state for the next instruction.
            PendingEnableInterrupt::AfterCurrentInstruction { .. } => {
                self.pending_enable_interrupts =
                    PendingEnableInterrupt::AfterCurrentInstruction { repeat: true };
            }
            // Already queued
            PendingEnableInterrupt::AfterNextInstruction => {}
        }
    }

    fn advance_pending_enable_interrupts_state(&mut self) {
        // Pending interrupts state only advances when an instruction finishes
        if self.ticks_to_next_instruction != 0 {
            return;
        }

        match self.pending_enable_interrupts {
            PendingEnableInterrupt::None => {}
            // An instruction just finished so we can enable interrupts now
            PendingEnableInterrupt::AfterCurrentInstruction { repeat } => {
                self.regs.set_interrupts_enabled(true);

                // We may be repeating if there were multiple `ei` instructions back-to-back
                // so stay in this state for one more instruction if so.
                if repeat {
                    self.pending_enable_interrupts =
                        PendingEnableInterrupt::AfterCurrentInstruction { repeat: false };
                } else {
                    self.pending_enable_interrupts = PendingEnableInterrupt::None;
                }
            }
            // The `ei` instruction just finished, so we need to wait one more instruction
            PendingEnableInterrupt::AfterNextInstruction => {
                self.pending_enable_interrupts =
                    PendingEnableInterrupt::AfterCurrentInstruction { repeat: false };
            }
        }
    }

    /// Return the lower 5 bits of IE & IF. If nonzero then there is an interrupt to handle.
    pub fn interrupt_bits(&self) -> u8 {
        self.ie & self.if_reg() & 0x1F
    }

    fn handle_interrupt(&mut self, interrupt: Interrupt) {
        self.schedule_next_instruction(20);

        // Clear the interrupt flag for this interrupt and disable all interrupts while handler runs
        self.write_if_reg(self.if_reg() & !interrupt.flag_bit());
        self.regs.set_interrupts_enabled(false);

        self.call_interrupt_handler(interrupt);
    }

    /// Start an OAM DMA transfer. Data will be written to OAM at once when the transfer completes
    /// in a fixed number of ticks.
    ///
    /// Panics if an OAM DMA transfer is already in progress.
    pub fn start_oam_dma_transfer(&mut self, source_address: u16) {
        if self.current_oam_dma_transfer.is_some() {
            panic!("Attempted to start OAM DMA transfer while one is already in progress");
        }

        self.current_oam_dma_transfer = Some(OamDmaTransfer {
            source_address,
            ticks_remaining: OAM_DMA_TRANSFER_TICKS,
        });
    }

    /// Complete an OAM DMA transfer, actually writing all data to OAM.
    fn complete_oam_dma_transfer(&mut self) {
        let transfer = self.current_oam_dma_transfer.take().unwrap();
        let source_address = transfer.source_address;
        debug_assert!(transfer.ticks_remaining == 0);

        for i in 0..OAM_SIZE {
            let byte = self.read_address(source_address.wrapping_add(i as u16));
            self.oam[i] = byte;
        }
    }

    /// Advance the state of the current OAM DMA transfer each tick, if one is in progress.
    fn advance_oam_dma_transfer_state(&mut self) {
        if let Some(transfer) = &mut self.current_oam_dma_transfer {
            if transfer.ticks_remaining == 0 {
                self.complete_oam_dma_transfer();
                return;
            }

            let is_double_speed = self.is_double_speed();
            let transfer = self.current_oam_dma_transfer.as_mut().unwrap();

            // OAM DMA transfers run twice as fast in double speed mode
            if is_double_speed {
                transfer.ticks_remaining = transfer.ticks_remaining.saturating_sub(2);
            } else {
                transfer.ticks_remaining -= 1;
            }
        }
    }

    pub fn start_general_purpose_vram_dma_transfer(
        &mut self,
        source_address: Address,
        dest_address: Address,
        num_blocks: u8,
    ) {
        // General purpose transfers stop the CPU until complete
        let num_ticks = (num_blocks as u16) * VRAM_DMA_TRANSFER_TICKS_PER_BLOCK as u16;
        self.current_general_purpose_vram_dma_transfer = Some(num_ticks as usize);
        self.is_cpu_stopped_for_vram_dma = true;

        // This means it is not observable so we can perform the entire transfer at once.
        for i in 0..((num_blocks as u16) * VRAM_DMA_TRANSFER_BLOCK_SIZE) {
            let byte = self.read_address(source_address.wrapping_add(i as u16));
            self.write_address(dest_address.wrapping_add(i as u16), byte);
        }
    }

    fn advance_general_purpose_vram_dma_transfer_state(&mut self) {
        if let Some(num_ticks_remaining) = self.current_general_purpose_vram_dma_transfer.as_mut() {
            let num_ticks_remaining = *num_ticks_remaining;

            // Transfer is complete. CPU is resumed and HDMA5 is set to 0xFF.
            if num_ticks_remaining == 0 {
                self.is_cpu_stopped_for_vram_dma = false;
                self.current_general_purpose_vram_dma_transfer = None;
                self.write_hdma5_raw(0xFF);
                return;
            }

            self.current_general_purpose_vram_dma_transfer = Some(num_ticks_remaining - 1);
        }
    }

    pub fn has_active_hblank_vram_dam_transfer(&self) -> bool {
        self.current_hblank_vram_dma_transfer.is_some()
    }

    pub fn terminate_hblank_vram_dma_transfer(&mut self) {
        self.current_hblank_vram_dma_transfer = None;

        // Toggle the top bit of HDMA5 to indicate transfer has stopped while keeping the length
        // part of the register unchanged.
        self.write_hdma5_raw(self.hdma5() | 0x80);
    }

    pub fn start_hblank_vram_dma_transfer(
        &mut self,
        source: Address,
        dest: Address,
        num_blocks: u8,
    ) {
        self.current_hblank_vram_dma_transfer = Some(VramDmaTransfer {
            source,
            dest,
            remaining_ticks_in_current_hblank: None,
            num_blocks_left: num_blocks,
            total_num_blocks: num_blocks,
        });
    }

    fn start_hblank_vram_dma_transfer_block(&mut self) {
        // HBlank VRAM DMA transfers are paused if the CPU is halted
        if self.is_cpu_halted {
            return;
        }

        let transfer = self.current_hblank_vram_dma_transfer.as_mut().unwrap();

        let block_offset = (transfer.total_num_blocks - transfer.num_blocks_left) as u16
            * VRAM_DMA_TRANSFER_BLOCK_SIZE;
        let source_block_start = transfer.source + block_offset;
        let dest_block_start = transfer.dest + block_offset;

        // Perform a single block transfer
        for i in 0..VRAM_DMA_TRANSFER_BLOCK_SIZE {
            let byte = self.read_address(source_block_start + i);
            self.write_address(dest_block_start + i, byte);
        }

        // Update state to reflect completed block
        let transfer = self.current_hblank_vram_dma_transfer.as_mut().unwrap();
        transfer.num_blocks_left -= 1;

        // Stop the CPU while the transfer is in progress
        transfer.remaining_ticks_in_current_hblank = Some(VRAM_DMA_TRANSFER_TICKS_PER_BLOCK);
        self.is_cpu_stopped_for_vram_dma = true;
    }

    pub fn advance_hblank_vram_dma_transfer_state(&mut self) {
        if self.current_hblank_vram_dma_transfer.is_none() {
            return;
        }

        // HBlank VRAM DMA transfers are paused if the CPU is halted
        if self.is_cpu_halted {
            return;
        }

        let transfer = self.current_hblank_vram_dma_transfer.as_mut().unwrap();

        if transfer.remaining_ticks_in_current_hblank == Some(0) {
            transfer.remaining_ticks_in_current_hblank = None;

            // This block is complete so resume the CPU
            self.is_cpu_stopped_for_vram_dma = false;

            // Encode the number of blocks left in HDMA5
            let num_blocks_left = transfer.num_blocks_left;
            self.write_hdma5_raw(num_blocks_left & 0x7F);

            // Transfer is complete
            if num_blocks_left == 0 {
                self.current_hblank_vram_dma_transfer = None;
                self.write_hdma5_raw(0xFF);
                return;
            }
        } else if let Some(remaining_ticks) = transfer.remaining_ticks_in_current_hblank.as_mut() {
            *remaining_ticks -= 1;
        }
    }

    pub fn start_speed_switch(&mut self) {
        self.current_speed_switch = Some(SPEED_SWITCH_TICKS);
        self.halt_cpu();
    }

    fn advance_speed_switch_state(&mut self) {
        if let Some(ticks_remaining) = self.current_speed_switch.as_mut() {
            if *ticks_remaining == 0 {
                self.resume_halted_cpu();
                return;
            }

            *ticks_remaining -= 1;
        }
    }

    fn increment_timers(&mut self) {
        // Divider register is incremented every tick but only top byte is exposed via DIV register
        let old_divider = self.full_divider_register;
        let div_apu_falling_edge_mask;

        // Divider register increments twice as fast in double speed mode
        if self.is_double_speed() {
            self.full_divider_register = self.full_divider_register.wrapping_add(2);
            div_apu_falling_edge_mask = 0x2000;
        } else {
            self.full_divider_register = self.full_divider_register.wrapping_add(1);
            div_apu_falling_edge_mask = 0x1000;
        }

        let falling_edges = old_divider & !self.full_divider_register;

        // Increment timer if there was falling edge on the TAC-selected bit of the divider register
        let has_tac_falling_edge = (falling_edges & self.tac_mask) != 0;
        if has_tac_falling_edge && self.is_timer_enabled {
            let (new_tima, overflowed) = self.tima().overflowing_add(1);

            // Reset to TMA and generate an interrupt when timer overflows
            if overflowed {
                self.write_tima(self.tma());
                self.request_interrupt(Interrupt::Timer);
            } else {
                self.write_tima(new_tima);
            }
        }

        // Increment APU divider if there was a falling edge on the appropriate bit
        let has_div_apu_falling_edge = (falling_edges & div_apu_falling_edge_mask) != 0;
        if has_div_apu_falling_edge {
            self.apu_mut().advance_div_apu();
        }
    }

    /// Most initialization is emulated statically by setting the initial state. Perform any dynamic
    /// initialization here.
    fn emulate_boot_sequence(&mut self) {
        if self.is_cgb_machine() {
            if self.cartridge().is_cgb() {
                self.write_key0(self.cartridge().cgb_byte());
            } else {
                self.write_key0(0x04);
                self.write_opri(0x01);

                // TODO: Write compatibility palette
            }
        }

        // Unmap the boot ROM by writing A to the bank register
        self.write_bank(self.regs().a());
    }

    /// Sample the current audio channels and push to current frame's sample queue
    fn push_next_sample(&mut self) {
        let (left, right) = self.apu().sample_audio();
        self.audio_sample_queue.push_back(TimedSample {
            left,
            right,
            tick: self.tick,
        });
    }
}

/// Convert a duration to nanoseconds, assuming it fits in u64.
fn duration_to_nanos(duration: Duration) -> u64 {
    let seconds = duration.as_secs();
    let subsec_nanos = duration.subsec_nanos() as u64;
    seconds * 1_000_000_000 + subsec_nanos
}
