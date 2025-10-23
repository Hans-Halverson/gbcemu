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

    pub fn vbk(&self) -> Register {
        self.registers[offset(VBK)]
    }

    pub fn wbk(&self) -> Register {
        self.registers[offset(WBK)]
    }
}

/// Offset in the IO registers file. Simply the lower byte of the address.
const fn offset(address: Address) -> usize {
    (address & 0xFF) as usize
}

const VBK: Address = 0xFF4F;
const WBK: Address = 0xFF70;

pub const DMG_INIT_IO_REGISTERS: IoRegisterFile = [0xFF; IO_REGISTERS_SIZE];
pub const CGB_INIT_IO_REGISTERS: IoRegisterFile = [0xFF; IO_REGISTERS_SIZE];
