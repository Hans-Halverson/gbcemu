/// GameBoy has a 16-bit address space
pub type Address = u16;

/// First ROM bank 0x0000-0x4000
pub const FIRST_ROM_BANK_END: Address = 0x4000;

/// Second ROM bank 0x4000-0x8000
pub const ROM_START: Address = 0x4000;
pub const ROM_END: Address = 0x8000;
pub const ROM_BANK_SIZE: usize = (ROM_END - ROM_START) as usize;

/// VRAM bank 0x8000-0xA000
pub const VRAM_START: Address = 0x8000;
pub const VRAM_END: Address = 0xA000;
pub const SINGLE_VRAM_BANK_SIZE: usize = (VRAM_END - VRAM_START) as usize;

/// External RAM bank 0xA000-0xC000
pub const EXTERNAL_RAM_START: Address = 0xA000;
pub const EXTERNAL_RAM_END: Address = 0xC000;
pub const SINGLE_EXTERNAL_RAM_BANK_SIZE: usize = (EXTERNAL_RAM_END - EXTERNAL_RAM_START) as usize;

/// First work RAM bank 0xC000-0xD000
pub const FIRST_WORK_RAM_BANK_START: Address = 0xC000;
pub const FIRST_WORK_RAM_BANK_END: Address = 0xD000;

/// Second work RAM bank 0xD000-0xE000
pub const SECOND_WORK_RAM_BANK_START: Address = 0xD000;
pub const SECOND_WORK_RAM_BANK_END: Address = 0xE000;

pub const SINGLE_WORK_RAM_BANK_SIZE: usize =
    (FIRST_WORK_RAM_BANK_END - FIRST_WORK_RAM_BANK_START) as usize;
pub const TOTAL_WORK_RAM_SIZE: usize = SINGLE_WORK_RAM_BANK_SIZE * 2;

// Echo RAM 0xE000-0xFE00
pub const ECHO_RAM_END: Address = 0xFE00;

/// OAM (Object Attribute Memory) 0xFE00-0xFEA0
pub const OAM_START: Address = 0xFE00;
pub const OAM_END: Address = 0xFEA0;
pub const OAM_SIZE: usize = (OAM_END - OAM_START) as usize;

// Unusable space 0xFEA0-0xFF00

/// IO Registers 0xFF00-0xFF80
pub const IO_REGISTERS_START: Address = 0xFF00;
pub const IO_REGISTERS_END: Address = 0xFF80;
pub const IO_REGISTERS_SIZE: usize = (IO_REGISTERS_END - IO_REGISTERS_START) as usize;

/// HRAM (High RAM) 0xFF80-0xFFFF
pub const HRAM_START: Address = 0xFF80;
pub const HRAM_END: Address = 0xFFFF;
pub const HRAM_SIZE: usize = (HRAM_END - HRAM_START) as usize;

/// Interrupt Enable Register 0xFFFF
pub const IE_ADDRESS: Address = 0xFFFF;
