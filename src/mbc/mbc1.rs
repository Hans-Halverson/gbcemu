use crate::{
    address_space::{
        Address, EXTERNAL_RAM_START, FIRST_ROM_BANK_END, ROM_BANK_SIZE,
        SINGLE_EXTERNAL_RAM_BANK_SIZE,
    },
    mbc::mbc::{Location, Mbc, MbcKind, RegisterHandle},
};

pub struct Mbc1 {
    /// RAM Enable Register (0000–1FFF)
    is_ram_enabled: bool,
    /// ROM Bank Number, 5 bits (2000–3FFF)
    rom_bank_num: usize,
    /// RAM Bank Number or Upper Bits of ROM Bank Number (4000–5FFF)
    ///
    /// Two bit register that both:
    /// - Selects the RAM bank
    /// - Provides the upper two bits for the ROM bank number
    ram_bank_num_or_upper_bits: usize,
    /// Banking Mode Select (6000–7FFF)
    is_advanced_banking_mode: bool,
}

impl Mbc1 {
    pub fn new() -> Self {
        Mbc1 {
            is_ram_enabled: false,
            rom_bank_num: 1,
            ram_bank_num_or_upper_bits: 0,
            is_advanced_banking_mode: false,
        }
    }
}

const RAM_ENABLE_REGISTER: RegisterHandle = 0;
const ROM_BANK_NUMBER_REGISTER: RegisterHandle = 1;
const RAM_BANK_NUMBER_OR_UPPER_BITS_REGISTER: RegisterHandle = 2;
const BANKING_MODE_SELECT_REGISTER: RegisterHandle = 3;

/// Treat reads or writes to uninitialized RAM value register as reading/writing from a register
/// that always returns 0xFF/is ignored respectively.
const UNITIALIZED_RAM_VALUE_REGISTER: RegisterHandle = 4;

impl Mbc1 {
    fn first_rom_bank_number(&self) -> usize {
        if self.is_advanced_banking_mode {
            self.ram_bank_num_or_upper_bits << 5
        } else {
            0
        }
    }

    fn second_rom_bank_number(&self) -> usize {
        self.rom_bank_num + (self.ram_bank_num_or_upper_bits << 5)
    }

    fn ram_bank_number(&self) -> usize {
        if self.is_advanced_banking_mode {
            self.ram_bank_num_or_upper_bits
        } else {
            0
        }
    }

    /// Address expected to be in the range 0x0000-0x4000
    fn physical_first_rom_bank_address(bank_num: usize, addr: Address) -> usize {
        let physical_bank_start_offset = bank_num * ROM_BANK_SIZE;
        let offset_in_bank = addr as usize;
        let physical_addr = physical_bank_start_offset + offset_in_bank;

        physical_addr
    }

    /// Address expected to be in the range 0x4000-0x8000
    fn physical_second_rom_bank_address(bank_num: usize, addr: Address) -> usize {
        Self::physical_first_rom_bank_address(bank_num - 1, addr)
    }

    /// Address expected to be in the range 0xA000-0xC000
    fn physical_ram_bank_address(bank_num: usize, addr: Address) -> usize {
        let physical_bank_start_offset = bank_num * SINGLE_EXTERNAL_RAM_BANK_SIZE;
        let offset_in_bank = (addr - EXTERNAL_RAM_START) as usize;
        let physical_addr = physical_bank_start_offset + offset_in_bank;

        physical_addr
    }

    fn map_ram_address(&self, addr: Address) -> Location {
        if !self.is_ram_enabled {
            return Location::Register(UNITIALIZED_RAM_VALUE_REGISTER);
        }

        Location::Address(Self::physical_ram_bank_address(
            self.ram_bank_number(),
            addr,
        ))
    }
}

impl Mbc for Mbc1 {
    fn kind(&self) -> MbcKind {
        MbcKind::Mbc1
    }

    fn map_read_rom_address(&self, addr: Address) -> usize {
        if addr < FIRST_ROM_BANK_END {
            Self::physical_first_rom_bank_address(self.first_rom_bank_number(), addr)
        } else {
            Self::physical_second_rom_bank_address(self.second_rom_bank_number(), addr)
        }
    }

    fn map_write_rom_address(&self, addr: Address) -> Location {
        match addr {
            0..0x2000 => Location::Register(RAM_ENABLE_REGISTER),
            0x2000..0x4000 => Location::Register(ROM_BANK_NUMBER_REGISTER),
            0x4000..0x6000 => Location::Register(RAM_BANK_NUMBER_OR_UPPER_BITS_REGISTER),
            0x6000..0x8000 => Location::Register(BANKING_MODE_SELECT_REGISTER),
            _ => unreachable!(),
        }
    }

    fn map_read_ram_address(&self, addr: Address) -> Location {
        self.map_ram_address(addr)
    }

    fn map_write_ram_address(&self, addr: Address) -> Location {
        self.map_ram_address(addr)
    }

    fn read_register(&self, reg: RegisterHandle) -> u8 {
        match reg {
            // The only readable register we need to implement is the unitialized RAM value
            // register, which always returns 0xFF
            UNITIALIZED_RAM_VALUE_REGISTER => 0xFF,
            _ => unreachable!(),
        }
    }

    fn write_register(&mut self, register: RegisterHandle, value: u8) {
        match register {
            // RAM is enabled by setting the lower nibble to 0xA, otherwise is disabled
            RAM_ENABLE_REGISTER => {
                self.is_ram_enabled = (value & 0xF) == 0xA;
            }
            // Only lower 5 bits of the value are used. Enforce that bank number 0 is remapped to 1
            // when written.
            ROM_BANK_NUMBER_REGISTER => {
                let mut bank_num = (value & 0x1F) as usize;
                if bank_num == 0 {
                    bank_num = 1;
                }
                self.rom_bank_num = bank_num;
            }
            // Only lower 2 bits of the value are used
            RAM_BANK_NUMBER_OR_UPPER_BITS_REGISTER => {
                self.ram_bank_num_or_upper_bits = (value & 0x3) as usize;
            }
            // Only lowest bit of the value is used
            BANKING_MODE_SELECT_REGISTER => {
                self.is_advanced_banking_mode = (value & 0x1) != 0;
            }
            // Writes to unitialized RAM are modeled as a write to a register that is ignored
            UNITIALIZED_RAM_VALUE_REGISTER => {}
            _ => unreachable!(),
        }
    }
}
