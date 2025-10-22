use crate::{
    address_space::Address,
    mbc::{mbc1::Mbc1, no_mbc::NoMbc},
};

/// Memory Bank Controllers map the ROM and RAM banks into the GameBoy's address space.
///
/// The may contain internal registers or additional features.
pub trait Mbc {
    fn kind(&self) -> MbcKind;

    /// Map reads from addresses in the ROM area (0000-7FFF)
    fn map_read_rom_address(&self, addr: Address) -> usize;

    /// Map reads from addresses in the RAM area (A000-BFFF)
    fn map_read_ram_address(&self, addr: Address) -> Location;

    /// Map writes to addresses in the ROM area (0000-7FFF)
    fn map_write_rom_address(&self, addr: Address) -> Location;

    /// Map writes to addresses in the RAM area (A000-BFFF)
    fn map_write_ram_address(&self, addr: Address) -> Location;

    /// Read a byte from a register in the MBC
    fn read_register(&self, reg: RegisterHandle) -> u8;

    /// Write a byte to a register in the MBC
    fn write_register(&mut self, reg: RegisterHandle, value: u8);
}

#[derive(Clone, Copy, PartialEq)]
pub enum MbcKind {
    /// Cartridges without a Memory Bank Controller
    None,
    Mbc1,
}

pub fn create_mbc(kind: MbcKind) -> Box<dyn Mbc> {
    match kind {
        MbcKind::None => Box::new(NoMbc),
        MbcKind::Mbc1 => Box::new(Mbc1::new()),
    }
}

/// Opaque handle to a register in the MBC
pub type RegisterHandle = usize;

/// Physical location that an address maps to.
pub enum Location {
    /// Maps to a register in the MBC
    Register(RegisterHandle),
    /// Maps to a physical address in ROM or RAM
    Address(usize),
}
