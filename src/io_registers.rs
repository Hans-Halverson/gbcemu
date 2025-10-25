use concat_idents::concat_idents;

use crate::{
    address_space::{Address, IO_REGISTERS_SIZE},
    emulator::Register,
    machine::Machine,
};

/// File containing all IO registers.
///
/// Only handles the values directly read/written from memory, i.e. only handles masking.
pub struct IoRegisters {
    registers: IoRegisterFile,
}

pub type IoRegisterFile = [Register; IO_REGISTERS_SIZE];

impl IoRegisters {
    pub fn init_for_machine(machine: Machine) -> Self {
        let registers = match machine {
            Machine::Dmg => DMG_INIT_IO_REGISTERS,
            Machine::Cgb => CGB_INIT_IO_REGISTERS,
        };

        Self { registers }
    }

    pub fn read_register(&self, address: Address) -> Register {
        self.registers[offset(address)]
    }

    pub fn write_register(&mut self, address: Address, value: Register) {
        self.registers[offset(address)] = value;
    }

    pub fn lcdc_window_tile_map_number(&self) -> u8 {
        self.lcdc() & 0x40
    }

    pub fn lcdc_window_enable(&self) -> bool {
        self.lcdc() & 0x20 != 0
    }

    pub fn lcdc_bg_window_tile_data_area(&self) -> u8 {
        self.lcdc() & 0x10
    }

    pub fn lcdc_bg_tile_map_number(&self) -> u8 {
        self.lcdc() & 0x08
    }

    pub fn lcdc_obj_enable(&self) -> bool {
        self.lcdc() & 0x02 != 0
    }

    pub fn lcdc_bg_window_enable(&self) -> bool {
        self.lcdc() & 0x01 != 0
    }
}

/// Offset in the IO registers file. Simply the lower byte of the address.
const fn offset(address: Address) -> usize {
    (address & 0xFF) as usize
}

macro_rules! define_registers {
    ($(($name:ident, $addr:expr, $dmg_init:expr, $cgb_init:expr)),*,) => {
        impl IoRegisters {
            $(
                /// Read the raw value of the $name register.
                pub fn $name(&self) -> Register {
                    self.read_register($addr)
                }


                concat_idents!(fn_name = write_, $name {
                    /// Write a value directly to the $name register.
                    #[allow(unused)]
                    pub fn fn_name(&mut self, value: Register) {
                        self.write_register($addr, value);
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
    };
}

/// Register is not present on this system
const NONE: u8 = 0xFF;

/// Register has an arbitrary initial value (depends on boot ROM's duration)
const VARIABLE: u8 = 0xFF;

/// Register is unitialized
const UNITIALIZED: u8 = 0xFF;

define_registers!(
    (lcdc, 0xFF40, 0x91, 0x91),
    (scy, 0xFF42, NONE, 0x00),
    (scx, 0xFF43, NONE, 0x00),
    (wy, 0xFF4A, NONE, 0x00),
    (wx, 0xFF4B, NONE, 0x00),
    (ly, 0xFF44, 0x91, VARIABLE),
    (bgp, 0xFF47, 0xFC, 0xFC),
    (obp0, 0xFF48, UNITIALIZED, UNITIALIZED),
    (obp1, 0xFF49, UNITIALIZED, UNITIALIZED),
    (vbk, 0xFF4F, NONE, 0xFE),
    (wbk, 0xFF70, NONE, 0xF8),
);
