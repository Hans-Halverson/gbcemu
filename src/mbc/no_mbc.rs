use crate::{
    address_space::{Address, EXTERNAL_RAM_START},
    mbc::mbc::{Location, Mbc, MbcKind, RegisterHandle},
};

pub struct NoMbc;

impl NoMbc {
    fn physical_ram_address(addr: Address) -> Location {
        // Model RAM as still mapped to a an 8KB external bank
        let offset_in_ram = (addr - EXTERNAL_RAM_START) as usize;
        Location::Address(offset_in_ram)
    }
}

impl Mbc for NoMbc {
    fn kind(&self) -> MbcKind {
        MbcKind::None
    }

    fn map_read_rom_address(&self, addr: Address) -> usize {
        addr as usize
    }

    fn map_read_ram_address(&self, addr: Address) -> Location {
        Self::physical_ram_address(addr)
    }

    fn map_write_rom_address(&self, addr: Address) -> Location {
        Location::Address(addr as usize)
    }

    fn map_write_ram_address(&self, addr: Address) -> Location {
        Self::physical_ram_address(addr)
    }

    fn read_register(&self, _: RegisterHandle) -> u8 {
        // No registers
        0
    }

    fn write_register(&mut self, _: RegisterHandle, _: u8) {
        // No registers
    }
}
