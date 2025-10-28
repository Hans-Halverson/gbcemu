use std::{
    array,
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicU32, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use eframe::egui::Color32;

use crate::{
    address_space::{
        Address, ECHO_RAM_END, EXTERNAL_RAM_END, FIRST_WORK_RAM_BANK_END,
        FIRST_WORK_RAM_BANK_START, HRAM_END, HRAM_SIZE, HRAM_START, IE_ADDRESS, IO_REGISTERS_END,
        OAM_END, OAM_SIZE, OAM_START, ROM_END, SECOND_WORK_RAM_BANK_END,
        SECOND_WORK_RAM_BANK_START, SINGLE_VRAM_BANK_SIZE, SINGLE_WORK_RAM_BANK_SIZE,
        TOTAL_WORK_RAM_SIZE, VRAM_END, VRAM_START,
    },
    cartridge::Cartridge,
    cpu::registers::Registers,
    initialization::IE_INIT,
    io_registers::IoRegisters,
    machine::Machine,
    mbc::mbc::Location,
    options::Options,
    ppu::{Color, WindowLineCounter, draw_scanline},
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

#[derive(Clone)]
pub struct SharedInputAdapter {
    pressed_buttons: Arc<AtomicU8>,
}

impl SharedInputAdapter {
    pub fn new() -> Self {
        Self {
            pressed_buttons: Arc::new(AtomicU8::new(0)),
        }
    }

    fn get_pressed_buttons(&self) -> u8 {
        self.pressed_buttons.load(Ordering::Relaxed)
    }

    pub fn set_pressed_buttons(&mut self, buttons: u8) {
        self.pressed_buttons.store(buttons, Ordering::Relaxed);
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
const REFRESH_RATE: f64 = 59.7;

/// Total number of scanlines including VBlank period. Larger than the height of the screen.
const NUM_VIRTUAL_SCANLINES: usize = 154;

const TICKS_PER_FRAME: usize = 70224;

const TICKS_PER_SCANLINE: usize = TICKS_PER_FRAME / NUM_VIRTUAL_SCANLINES;

/// Number of ticks in OAM Scan mode at the beginning of each scanline
const OAM_SCAN_TICKS: usize = 80;

/// Number of ticks in Draw mode. In reality this is variable, but we choose the minimum time here.
const DRAW_TICKS: usize = 172;

const HBLANK_TICKS: usize = TICKS_PER_SCANLINE - OAM_SCAN_TICKS - DRAW_TICKS;

/// Total number of ticks to complete an OAM DMA transfer
const OAM_DMA_TRANSFER_TICKS: usize = 640;

/// Nanoseconds in real time per frame
const NS_PER_FRAME: f64 = 1_000_000_000.0f64 / REFRESH_RATE;

#[derive(Clone, Copy)]
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
enum PendingEnableInterrupt {
    None,
    AfterNextInstruction,
    /// Set to repeat if two `ei` instructions are executed back-to-back.
    AfterCurrentInstruction {
        repeat: bool,
    },
}

struct OamDmaTransfer {
    /// The source address which data is copied from into OAM
    source_address: Address,
    /// The number of ticks until this transfer is complete
    ticks_remaining: usize,
}

pub struct Emulator {
    /// Cartridge inserted
    cartridge: Cartridge,

    /// Options for the emulator
    options: Arc<Options>,

    /// Input adapter for reading button presses from the GUI thread
    input_adapter: SharedInputAdapter,

    /// Output buffer for the screen which is shared with the GUI thread
    output_buffer: SharedOutputBuffer,

    /// Current tick (T-cycle) within a frame
    tick: u32,

    /// Current frame number
    frame: u64,

    /// Current (virtual) scanline number
    scanline: u8,

    /// Current mode
    mode: Mode,

    /// Whether the emulator is currently in CGB mode
    in_cgb_mode: bool,

    /// VRAM region, including all banks
    vram: Vec<u8>,

    /// OAM region (Object Attribute Memory)
    oam: Vec<u8>,

    /// HRAM region (High RAM)
    hram: Vec<u8>,

    /// Work RAM region, including all banks
    work_ram: Vec<u8>,

    /// Register file
    regs: Registers,

    /// IO register file
    io_regs: IoRegisters,

    /// Interrupt Enable register (0xFFFF) (IE)
    ie: Register,

    /// Number of ticks remaining until the next instruction is executed
    ticks_to_next_instruction: usize,

    /// State machine for `ei` instructions to enable interrupts after the next instruction
    pending_enable_interrupts: PendingEnableInterrupt,

    /// The current OAM DMA transfer, if one is in progress
    current_oam_dma_transfer: Option<OamDmaTransfer>,

    /// Whether the CPU is currently halted
    is_cpu_halted: bool,

    /// Internal line number counter used for rendering the window
    window_line_counter: WindowLineCounter,

    /// Set of currently pressed buttons, exposed via the joypad register
    pressed_buttons: ButtonSet,
}

impl Emulator {
    pub fn new(cartridge: Cartridge, machine: Machine, options: Arc<Options>) -> Self {
        let is_cgb = cartridge.is_cgb();

        Emulator {
            cartridge,
            options,
            input_adapter: SharedInputAdapter::new(),
            output_buffer: SharedOutputBuffer::new(),
            tick: 0,
            frame: 0,
            scanline: 0,
            mode: Mode::OamScan,
            in_cgb_mode: is_cgb,
            vram: vec![0; machine.vram_size()],
            oam: vec![0; OAM_SIZE],
            hram: vec![0; HRAM_SIZE],
            work_ram: vec![0; TOTAL_WORK_RAM_SIZE],
            regs: Registers::init_for_machine(machine),
            io_regs: IoRegisters::init_for_machine(machine),
            ie: IE_INIT,
            ticks_to_next_instruction: 0,
            pending_enable_interrupts: PendingEnableInterrupt::None,
            current_oam_dma_transfer: None,
            is_cpu_halted: false,
            window_line_counter: WindowLineCounter::new(),
            pressed_buttons: 0,
        }
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

    pub fn in_test_mode(&self) -> bool {
        self.options.in_test_mode
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

    pub fn halt_cpu(&mut self) {
        self.is_cpu_halted = true;
    }

    pub fn window_line_counter_mut(&mut self) -> &mut WindowLineCounter {
        &mut self.window_line_counter
    }

    pub fn pressed_buttons(&self) -> ButtonSet {
        self.pressed_buttons
    }

    pub fn clone_input_adapter(&self) -> SharedInputAdapter {
        self.input_adapter.clone()
    }

    pub fn clone_output_buffer(&self) -> SharedOutputBuffer {
        self.output_buffer.clone()
    }

    /// Run the emulator at the GameBoy's native framerate
    pub fn run(&mut self) {
        let start_time = Instant::now();

        loop {
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
            self.run_frame();

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
                    nanos_to_next_frame.saturating_sub(NS_PER_FRAME as u64 / 20),
                ));
            }
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
            self.set_mode(Mode::HBlank);
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

        // Ready for next instruction. Either execute the next instruction or an interrupt handler.
        'handled: {
            if self.ticks_to_next_instruction == 0 {
                let interrupt_bits = self.interrupt_bits();
                if interrupt_bits != 0 {
                    // A pending interrupts resumes a halted CPU, even if IME is disabled and
                    // interrup won't actually be handled.
                    self.is_cpu_halted = false;

                    if self.regs().interrupts_enabled() {
                        self.handle_interrupt(Interrupt::for_bits(interrupt_bits));
                        break 'handled;
                    }
                }

                if !self.is_cpu_halted {
                    self.execute_instruction();
                    break 'handled;
                }
            }
        }

        self.tick += 1;
        self.ticks_to_next_instruction = self.ticks_to_next_instruction.saturating_sub(1);

        // Advance states at the end of the tick
        self.advance_pending_enable_interrupts_state();
        self.advance_oam_dma_transfer_state();
    }

    fn handle_inputs(&mut self) {
        // Check if there are any different pressed buttons
        let new_pressed_buttons = self.input_adapter.get_pressed_buttons();
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

    pub fn write_color(&self, x: u8, y: u8, color: Color) {
        self.output_buffer.write_pixel(
            x as usize,
            y as usize,
            SCREEN_COLOR_PALETTE_GRAYSCALE[color as usize],
        );
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

    fn physical_vram_bank_address(&self, addr: Address) -> usize {
        let bank_num = if self.in_cgb_mode {
            (self.vbk() & 0x01) as usize
        } else {
            0
        };

        (addr - VRAM_START) as usize + bank_num * SINGLE_VRAM_BANK_SIZE
    }

    fn physical_first_work_ram_bank_address(&self, addr: Address) -> usize {
        (addr - FIRST_WORK_RAM_BANK_START) as usize
    }

    fn second_wram_bank_num(&self) -> usize {
        if self.in_cgb_mode {
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

        for i in 0..OAM_SIZE {
            let byte = self.read_address(source_address.wrapping_add(i as u16));
            self.oam[i] = byte;
        }
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
            transfer.ticks_remaining -= 1;

            if transfer.ticks_remaining == 0 {
                self.complete_oam_dma_transfer();
            }
        }
    }
}

/// Convert a duration to nanoseconds, assuming it fits in u64.
fn duration_to_nanos(duration: Duration) -> u64 {
    let seconds = duration.as_secs();
    let subsec_nanos = duration.subsec_nanos() as u64;
    seconds * 1_000_000_000 + subsec_nanos
}
