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
    machine: Machine,
}

pub type IoRegisterFile = [Register; IO_REGISTERS_SIZE];

impl IoRegisters {
    pub fn init_for_machine(machine: Machine) -> Self {
        let registers = match machine {
            Machine::Dmg => DMG_INIT_IO_REGISTERS,
            Machine::Cgb => CGB_INIT_IO_REGISTERS,
        };

        Self { registers, machine }
    }

    pub fn read_register_raw(&self, address: Address) -> Register {
        self.registers[offset(address)]
    }

    pub fn write_register_raw(&mut self, address: Address, value: Register) {
        self.registers[offset(address)] = value;
    }
}

/// Offset in the IO registers file. Simply the lower byte of the address.
const fn offset(address: Address) -> usize {
    (address & 0xFF) as usize
}

pub const DMG_INIT_IO_REGISTERS: IoRegisterFile = [0xFF; IO_REGISTERS_SIZE];
pub const CGB_INIT_IO_REGISTERS: IoRegisterFile = [0xFF; IO_REGISTERS_SIZE];

macro_rules! define_registers {
    ($(($name:ident, $addr:expr, $mask:expr, $default_bits:expr)),*,) => {

        impl IoRegisters {

            $(
                /// Read the value of the $name register with masking applied.
                ///
                /// A default value for the register will be returned in all non-masked bits.
                pub fn $name(&self) -> Register {
                    let default_bits = $default_bits & !$mask;
                    let real_bits = self.read_register_raw($addr) & $mask;
                    default_bits | real_bits
                }

                concat_idents!(fn_name = $name, _raw {
                    /// Read the raw value of the $name register without masking.
                    #[allow(unused)]
                    pub fn fn_name(&self) -> Register {
                        self.read_register_raw($addr)
                    }
                });

                concat_idents!(fn_name = write_, $name {
                    /// Write a value to the $name register with masking applied, meaning only the
                    /// masked bits will be set and all non-masked bits are unchanged.
                    #[allow(unused)]
                    pub fn fn_name(&mut self, value: Register) {
                        let unchanged_bits = self.read_register_raw($addr) & !$mask;
                        let new_bits = value & $mask;
                        self.write_register_raw($addr, unchanged_bits | new_bits);
                    }
                });

                concat_idents!(fn_name = write_, $name, _raw {
                    /// Write a value to the $name register without masking.
                    #[allow(unused)]
                    pub fn fn_name(&mut self, value: Register) {
                        self.write_register_raw($addr, value);
                    }
                });
            )*
        }
    };
}

const ONE_BIT_MASK: u8 = 0x1;
const THREE_BIT_MASK: u8 = 0x7;

const DEFAULT_ZEROES: u8 = 0x00;
const DEFAULT_ONES: u8 = 0xFF;

define_registers!(
    (vbk, 0xFF4F, ONE_BIT_MASK, DEFAULT_ONES),
    (wbk, 0xFF70, THREE_BIT_MASK, DEFAULT_ZEROES),
);
