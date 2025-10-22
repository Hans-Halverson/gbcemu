/// GameBoy has a 16-bit address space
pub type Address = u16;

pub const FIRST_ROM_BANK_END: Address = 0x4000;
pub const ROM_END: Address = 0x8000;
pub const VRAM_END: Address = 0xA000;

pub const EXTERNAL_RAM_START: Address = 0xA000;
pub const EXTERNAL_RAM_END: Address = 0xC000;

pub const ROM_BANK_SIZE: usize = 0x4000;
pub const RAM_BANK_SIZE: usize = 0x2000;
