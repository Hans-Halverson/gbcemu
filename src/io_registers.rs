use concat_idents::concat_idents;

use crate::{
    address_space::{Address, IO_REGISTERS_SIZE},
    emulator::{Emulator, Register},
    machine::Machine,
};

/// File containing all IO registers.
type IoRegisterFile = [Register; IO_REGISTERS_SIZE];

pub struct IoRegisters {
    registers: IoRegisterFile,
}

impl IoRegisters {
    pub fn init_for_machine(machine: Machine) -> Self {
        let registers = match machine {
            Machine::Dmg => DMG_INIT_IO_REGISTERS,
            Machine::Cgb => CGB_INIT_IO_REGISTERS,
        };

        Self { registers }
    }

    fn as_slice(&self) -> &IoRegisterFile {
        &self.registers
    }

    fn as_slice_mut(&mut self) -> &mut IoRegisterFile {
        &mut self.registers
    }
}

impl Emulator {
    /// Read an IO register, applying any special behavior.
    ///
    /// Address must be in the IO register range (0xFF00-0xFF80).
    pub fn read_io_register(&self, address: Address) -> Register {
        let offset = offset(address);
        let read_handler = READ_HANDLERS[offset];

        read_handler(self, address)
    }

    /// Read a full byte without modification.
    fn read_register_raw(&self, address: Address) -> Register {
        self.io_regs().as_slice()[offset(address)]
    }

    /// Write an IO register, applying any special behavior.
    ///
    /// Address must be in the IO register range (0xFF00-0xFF80).
    pub fn write_io_register(&mut self, address: Address, value: Register) {
        let offset = offset(address);
        let write_handler = WRITE_HANDLERS[offset];

        write_handler(self, address, value)
    }

    /// Write a full byte without modification.
    fn write_register_raw(&mut self, address: Address, value: Register) {
        self.io_regs_mut().as_slice_mut()[offset(address)] = value;
    }

    fn write_to_read_only_register(&mut self, address: Address, _: Register) {
        panic!(
            "Attempted to write to read-only register at address {:04X}",
            address
        );
    }

    pub fn is_lcdc_lcd_enabled(&self) -> bool {
        self.lcdc_raw() & 0x80 != 0
    }

    pub fn lcdc_window_tile_map_number(&self) -> u8 {
        self.lcdc_raw() & 0x40
    }

    pub fn is_lcdc_window_enabled(&self) -> bool {
        self.lcdc_raw() & 0x20 != 0
    }

    pub fn lcdc_bg_window_tile_data_area(&self) -> u8 {
        self.lcdc_raw() & 0x10
    }

    pub fn lcdc_bg_tile_map_number(&self) -> u8 {
        self.lcdc_raw() & 0x08
    }

    pub fn is_lcdc_obj_enabled(&self) -> bool {
        self.lcdc_raw() & 0x02 != 0
    }

    pub fn is_lcdc_bg_window_enabled(&self) -> bool {
        self.lcdc_raw() & 0x01 != 0
    }

    pub fn is_stat_hblank_interrupt_enabled(&self) -> bool {
        self.stat_raw() & 0x08 != 0
    }

    pub fn is_stat_vblank_interrupt_enabled(&self) -> bool {
        self.stat_raw() & 0x10 != 0
    }

    pub fn is_stat_oam_scan_interrupt_enabled(&self) -> bool {
        self.stat_raw() & 0x20 != 0
    }

    pub fn is_stat_lyc_interrupt_enabled(&self) -> bool {
        self.stat_raw() & 0x40 != 0
    }

    fn write_if_impl(&mut self, _: Address, value: Register) {
        // Write the lower 5 bits, leave the top 3 set. This allows raw reads.
        self.write_if_reg_raw(0xE0 | (0x1F & value));
    }

    fn read_lcd_stat_impl(&self, _: Address) -> Register {
        // Construct the STAT register value on reads, allowing for raw writes.
        let raw = self.stat_raw();

        let unused_bits = 0x80;
        let interrupt_bits = raw | 0x78;

        let lyc_bit = if self.scanline() == self.lyc() {
            0x04
        } else {
            0x00
        };

        let mode_bits = self.mode().byte_value();

        unused_bits | interrupt_bits | lyc_bit | mode_bits
    }

    fn write_lyc_impl(&mut self, _: Address, value: Register) {
        self.write_lyc_raw(value);

        // Request interrupt for LYC=LY if needed
        if value == self.scanline() && self.is_stat_lyc_interrupt_enabled() {
            self.request_interrupt(crate::emulator::Interrupt::LcdStat);
        }
    }

    fn read_ly_impl(&self, _: Address) -> Register {
        self.scanline()
    }

    fn write_vbk_impl(&mut self, _: Address, value: Register) {
        // Only write bottom bit, leaving top 7 bits set. This allows raw reads.
        self.write_vbk_raw(0xFE | (0x01 & value));
    }

    fn write_wbk_impl(&mut self, _: Address, value: Register) {
        // Only write bottom 3 bits, leaving top 5 bits set. This allows raw reads.
        // Value 0 is treated as 1.
        self.write_wbk_raw(0xF8 | ((0x07 & value).max(1)));
    }
}

/// Offset in the IO registers file. Simply the lower byte of the address.
const fn offset(address: Address) -> usize {
    (address & 0xFF) as usize
}

macro_rules! define_registers {
    ($(($name:ident, $addr:expr, $dmg_init:expr, $cgb_init:expr, $read_fn:ident, $write_fn:ident)),*,) => {
        impl Emulator {
            $(
                /// Read the a value from the $name, applying any special behavior.
                pub fn $name(&self) -> Register {
                    self.$read_fn($addr)
                }

                concat_idents!(fn_name = write_, $name {
                    /// Write a value to the $name register, applying any special behavior.
                    #[allow(unused)]
                    pub fn fn_name(&mut self, value: Register) {
                        self.$write_fn($addr, value);
                    }
                });

                concat_idents!(fn_name = $name, _raw {
                    /// Read a raw byte directly from the $name register.
                    #[allow(unused)]
                    fn fn_name(&self) -> Register {
                        self.read_register_raw($addr)
                    }
                });

                concat_idents!(fn_name = write_, $name, _raw {
                    /// Write a raw byte directly to the $name register.
                    #[allow(unused)]
                    pub fn fn_name(&mut self, value: Register) {
                        self.write_register_raw($addr, value);
                    }
                });
            )*
        }

        const DMG_INIT_IO_REGISTERS: IoRegisterFile = const {
            let mut registers = [0xFF; IO_REGISTERS_SIZE];
            $(
                registers[offset($addr)] = $dmg_init;
            )*
            registers
        };

        const CGB_INIT_IO_REGISTERS: IoRegisterFile = const {
            let mut registers = [0xFF; IO_REGISTERS_SIZE];
            $(
                registers[offset($addr)] = $cgb_init;
            )*
            registers
        };

        const READ_HANDLERS: [fn(&Emulator, Address) -> Register; IO_REGISTERS_SIZE] = const {
            let mut handlers: [fn(&Emulator, Address) -> Register; IO_REGISTERS_SIZE] =
                [Emulator::read_register_raw; IO_REGISTERS_SIZE];

            $(
                handlers[offset($addr)] = Emulator::$read_fn;
            )*

            handlers
        };

        const WRITE_HANDLERS: [fn(&mut Emulator, Address, Register); IO_REGISTERS_SIZE] = const {
            let mut handlers: [fn(&mut Emulator, Address, Register); IO_REGISTERS_SIZE] =
                [Emulator::write_register_raw; IO_REGISTERS_SIZE];

            $(
                handlers[offset($addr)] = Emulator::$write_fn;
            )*

            handlers
        };
    }
}

/// Register is not present on this system
const NONE: u8 = 0xFF;

/// Register has an arbitrary initial value (depends on boot ROM's duration)
const VARIABLE: u8 = 0xFF;

/// Register is unitialized
const UNITIALIZED: u8 = 0xFF;

// (register name, address, DMG initial value, CGB initial value, read handler, write handler)
define_registers!(
    (if_reg, 0xFF0F, 0xE1, 0xE1, read_register_raw, write_if_impl),
    (
        lcdc,
        0xFF40,
        0x91,
        0x91,
        read_register_raw,
        write_register_raw
    ),
    (
        stat,
        0xFF41,
        0x85,
        VARIABLE,
        read_lcd_stat_impl,
        write_register_raw
    ),
    (
        scy,
        0xFF42,
        NONE,
        0x00,
        read_register_raw,
        write_register_raw
    ),
    (
        scx,
        0xFF43,
        NONE,
        0x00,
        read_register_raw,
        write_register_raw
    ),
    (lyc, 0xFF45, 0x0, 0x00, read_register_raw, write_lyc_impl),
    (
        wy,
        0xFF4A,
        NONE,
        0x00,
        read_register_raw,
        write_register_raw
    ),
    (
        wx,
        0xFF4B,
        NONE,
        0x00,
        read_register_raw,
        write_register_raw
    ),
    (
        ly,
        0xFF44,
        0x91,
        VARIABLE,
        read_ly_impl,
        write_to_read_only_register
    ),
    (
        bgp,
        0xFF47,
        0xFC,
        0xFC,
        read_register_raw,
        write_register_raw
    ),
    (
        obp0,
        0xFF48,
        UNITIALIZED,
        UNITIALIZED,
        read_register_raw,
        write_register_raw
    ),
    (
        obp1,
        0xFF49,
        UNITIALIZED,
        UNITIALIZED,
        read_register_raw,
        write_register_raw
    ),
    (vbk, 0xFF4F, NONE, 0xFE, read_register_raw, write_vbk_impl),
    (wbk, 0xFF70, NONE, 0xF8, read_register_raw, write_wbk_impl),
);
