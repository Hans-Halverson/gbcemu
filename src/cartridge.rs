use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    address_space::{ROM_BANK_SIZE, SINGLE_EXTERNAL_RAM_BANK_SIZE},
    mbc::mbc::{Mbc, MbcKind, create_mbc},
};

struct Scanner<'a> {
    data: &'a [u8],
    /// Current position in the buffer
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn new(data: &'a [u8]) -> Self {
        Scanner { data, pos: 0 }
    }

    fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }

    fn read_u8(&mut self) -> u8 {
        let result = self.data[self.pos];
        self.pos += 1;
        result
    }

    fn skip(&mut self, len: usize) {
        self.pos += len;
    }

    fn read_bytes(&mut self, len: usize) -> &[u8] {
        let result = &self.data[self.pos..self.pos + len];
        self.pos += len;
        result
    }
}

#[rustfmt::skip]
const NINTENDO_LOGO: [u8; 48] = [
    0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D,
    0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99,
    0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E,
];

#[derive(Serialize, Deserialize)]
pub struct Cartridge {
    /// Raw ROM data
    #[serde(with = "serde_bytes")]
    rom: Vec<u8>,

    /// RAM for this cartridge
    #[serde(with = "serde_bytes")]
    ram: Vec<u8>,

    /// Memory Bank Controller for this cartridge
    mbc: Box<dyn Mbc>,

    /// The code executed at startup.
    ///
    /// e.g. nop, jmp 0x0150
    entry_point_code: [u8; 4],

    /// Title of the ROM
    title: String,

    /// Cartridge type byte
    cartridge_type_byte: u8,

    /// CGB compatibility byte
    cgb_byte: u8,
}

impl Cartridge {
    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    pub fn rom_mut(&mut self) -> &mut [u8] {
        &mut self.rom
    }

    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    pub fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }

    pub fn mbc(&self) -> &dyn Mbc {
        self.mbc.as_ref()
    }

    pub fn mbc_mut(&mut self) -> &mut dyn Mbc {
        self.mbc.as_mut()
    }

    pub fn cgb_byte(&self) -> u8 {
        self.cgb_byte
    }

    pub fn is_cgb(&self) -> bool {
        self.cgb_byte & 0x80 != 0
    }

    pub fn new_from_rom_bytes(rom_bytes: Vec<u8>) -> Self {
        let mut scanner = Scanner::new(&rom_bytes);

        // Header starts at 0x0100
        scanner.seek(0x0100);

        // Entry point code (4 bytes)
        let entry_point_code = scanner.read_bytes(4).try_into().unwrap();

        // Must be followed by a bitmap of the Nintendo logo (48 bytes)
        let nintendo_logo = scanner.read_bytes(NINTENDO_LOGO.len());
        assert_eq!(nintendo_logo, NINTENDO_LOGO);

        // Title is ended by a null byte (16 bytes long)
        let title_bytes = scanner.read_bytes(11);
        let title = title_bytes
            .iter()
            .map(|b| *b as char)
            .take_while(|c| *c != '\0')
            .collect();

        // Skip manufacturer code (4 bytes)
        scanner.skip(4);

        // CGB flag (1 byte)
        let cgb_byte = scanner.read_u8();

        // Skip new licensee code (2 bytes)
        scanner.skip(2);

        // Skip SGB flag (1 byte)
        scanner.skip(1);

        // Skip cartridge type (1 byte),
        let cartridge_type_byte = scanner.read_u8();

        // ROM size (1 byte)
        let rom_size_byte = scanner.read_u8();
        assert!(rom_size_byte <= 0x08, "Unsupported ROM size");

        let rom_size = (2 * ROM_BANK_SIZE) << rom_size_byte;
        assert_eq!(rom_bytes.len(), rom_size, "ROM size mismatch");

        // Create MBC for this cartridge type
        let mbc_kind = Self::mbc_kind_for_cartridge_type(cartridge_type_byte);
        let mbc = create_mbc(mbc_kind, rom_size);

        // RAM size (1 byte)
        let ram_size_byte = scanner.read_u8();
        let mut ram_size = match ram_size_byte {
            // Still map 0x00 and 0x01 to 8KB of RAM as we have encountered test ROMS that expect
            // this.
            0x00 | 0x01 => SINGLE_EXTERNAL_RAM_BANK_SIZE,
            0x02 => SINGLE_EXTERNAL_RAM_BANK_SIZE,
            0x03 => 4 * SINGLE_EXTERNAL_RAM_BANK_SIZE,
            0x04 => 16 * SINGLE_EXTERNAL_RAM_BANK_SIZE,
            0x05 => 8 * SINGLE_EXTERNAL_RAM_BANK_SIZE,
            _ => panic!("Unsupported RAM size"),
        };

        // Treat no MBC as having 8KB of external RAM so that the MBC trait's mappings always map to
        // the cartridge's external RAM (for consistency).
        if mbc_kind == MbcKind::None {
            ram_size = 8 * 1024;
        }

        let ram = vec![0; ram_size];

        // Skip destination code (1 byte)
        scanner.skip(1);

        // Skip old licensee code (1 byte)
        scanner.skip(1);

        // Skip mask ROM version number (1 byte)
        scanner.skip(1);

        // Header checksum (1 byte)
        let header_checksum = scanner.read_u8();
        Self::validate_header_checksum(&rom_bytes, header_checksum);

        // Skip global checksum (2 bytes)
        scanner.skip(2);

        assert_eq!(scanner.pos, 0x0150, "Unexpected header size");

        Cartridge {
            rom: rom_bytes,
            ram,
            mbc,
            entry_point_code,
            title,
            cartridge_type_byte,
            cgb_byte,
        }
    }

    fn validate_header_checksum(data: &[u8], checksum: u8) {
        let mut sum: u8 = 0;
        for i in 0x0134..=0x014C {
            sum = sum.wrapping_sub(data[i]).wrapping_sub(1);
        }
        assert_eq!(sum, checksum, "Header checksum mismatch");
    }

    fn mbc_kind_for_cartridge_type(cartridge_type: u8) -> MbcKind {
        match cartridge_type {
            0x00 => MbcKind::None,
            0x01..=0x03 => MbcKind::Mbc1,
            0x0F..=0x13 => MbcKind::Mbc3,
            _ => panic!("Unsupported cartridge type: 0x{:02X}", cartridge_type),
        }
    }
}

impl fmt::Debug for Cartridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Cartridge {{
  entry_point_code: {:02X?},
  title: {},
  cartridge_type_byte: {:02X},
  is_cgb: {},
  rom_size: {},
  ram_size: {},
}}",
            self.entry_point_code,
            self.title,
            self.cartridge_type_byte,
            self.is_cgb(),
            self.rom.len(),
            self.ram.len()
        )
    }
}

unsafe impl Send for Cartridge {}
