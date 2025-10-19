use std::fmt;

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

pub struct Rom {
    /// Raw ROM data
    data: Vec<u8>,

    /// The code executed at startup.
    ///
    /// e.g. nop, jmp 0x0150
    entry_point_code: [u8; 4],

    /// Title of the ROM
    title: String,

    /// Cartridge type byte
    cartridge_type_byte: u8,

    /// Size of the ROM in bytes
    rom_size: usize,

    /// Size of the RAM in bytes
    ram_size: usize,
}

impl Rom {
    pub fn new_from_bytes(data: Vec<u8>) -> Rom {
        let mut scanner = Scanner::new(&data);

        // Header starts at 0x0100
        scanner.seek(0x0100);

        // Entry point code (4 bytes)
        let entry_point_code = scanner.read_bytes(4).try_into().unwrap();

        // Must be followed by a bitmap of the Nintendo logo (48 bytes)
        let nintendo_logo = scanner.read_bytes(NINTENDO_LOGO.len());
        assert_eq!(nintendo_logo, NINTENDO_LOGO);

        // Title is ended by a null byte (16 bytes long)
        let title_bytes = scanner.read_bytes(16);
        let title = title_bytes
            .iter()
            .map(|b| *b as char)
            .take_while(|c| *c != '\0')
            .collect();

        // Skip new licensee code (2 bytes)
        scanner.skip(2);

        // Skip SGB flag (1 byte)
        scanner.skip(1);

        // Skip cartridge type (1 byte),
        let cartridge_type_byte = scanner.read_u8();

        // ROM size (1 byte)
        let rom_size_byte = scanner.read_u8();
        assert!(rom_size_byte <= 0x08, "Unsupported ROM size");
        let rom_size = (32 * 1024) << rom_size_byte;

        // RAM size (1 byte)
        let ram_size_byte = scanner.read_u8();
        let ram_size = match ram_size_byte {
            0x00 | 0x01 => 0,
            0x02 => 8 * 1024,
            0x03 => 32 * 1024,
            0x04 => 128 * 1024,
            0x05 => 64 * 1024,
            _ => panic!("Unsupported RAM size"),
        };
        assert_eq!(data.len(), rom_size, "ROM size mismatch");

        // Skip destination code (1 byte)
        scanner.skip(1);

        // Skip old licensee code (1 byte)
        scanner.skip(1);

        // Skip mask ROM version number (1 byte)
        scanner.skip(1);

        // Header checksum (1 byte)
        let header_checksum = scanner.read_u8();
        Self::validate_header_checksum(&data, header_checksum);

        // Skip global checksum (2 bytes)
        scanner.skip(2);

        assert_eq!(scanner.pos, 0x0150, "Unexpected header size");

        Rom {
            data,
            entry_point_code,
            title,
            cartridge_type_byte,
            rom_size,
            ram_size,
        }
    }

    fn validate_header_checksum(data: &[u8], checksum: u8) {
        let mut sum: u8 = 0;
        for i in 0x0134..=0x014C {
            sum = sum.wrapping_sub(data[i]).wrapping_sub(1);
        }
        assert_eq!(sum, checksum, "Header checksum mismatch");
    }
}

impl fmt::Debug for Rom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Rom {{
  entry_point_code: {:02X?},
  title: {},
  cartridge_type_byte: {:02X},
  rom_size: {},
  ram_size: {},
}}",
            self.entry_point_code,
            self.title,
            self.cartridge_type_byte,
            self.rom_size,
            self.ram_size
        )
    }
}
