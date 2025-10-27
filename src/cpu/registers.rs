use crate::machine::Machine;

pub struct Registers {
    /// Program counter, address of the next instruction to execute
    pc: u16,

    /// Stack pointer
    sp: u16,

    /// Accumulator
    a: u8,

    /// General purpose registers
    bc: [u8; 2],
    de: [u8; 2],
    hl: [u8; 2],

    /// Set iff an operation is zero
    zero_flag: bool,

    /// Set when addition or subtraction overflows, or when a 1 bit is shifted out
    carry_flag: bool,

    /// BCD flags, not currently set
    n_flag: bool,
    h_flag: bool,

    /// Whether any interrupt are enabled, if disabled no interrupts will be handled
    interrupts_enabled: bool,
}

impl Registers {
    fn new_for_dmg() -> Self {
        Registers {
            pc: 0x0100,
            sp: 0xFFFE,
            a: 0x01,
            bc: [0x00, 0x13],
            de: [0x00, 0xD8],
            hl: [0x01, 0x4D],
            zero_flag: true,
            // Variable depending on header checksum, choose an arbitrary value
            carry_flag: false,
            // Variable depending on header checksum, choose an arbitrary value
            h_flag: false,
            n_flag: false,
            interrupts_enabled: false,
        }
    }

    fn new_for_cgb() -> Self {
        Registers {
            pc: 0x0100,
            sp: 0xFFFE,
            a: 0x11,
            bc: [0x00, 0x00],
            de: [0xFF, 0x56],
            hl: [0x00, 0x0D],
            zero_flag: true,
            carry_flag: false,
            h_flag: false,
            n_flag: false,
            interrupts_enabled: false,
        }
    }

    pub fn init_for_machine(machine: Machine) -> Self {
        match machine {
            Machine::Dmg => Self::new_for_dmg(),
            Machine::Cgb => Self::new_for_cgb(),
        }
    }

    pub fn pc(&self) -> u16 {
        self.pc
    }

    pub fn set_pc(&mut self, value: u16) {
        self.pc = value;
    }

    pub fn sp(&self) -> u16 {
        self.sp
    }

    pub fn set_sp(&mut self, value: u16) {
        self.sp = value;
    }

    pub fn a(&self) -> u8 {
        self.a
    }

    pub fn set_a(&mut self, value: u8) {
        self.a = value;
    }

    pub fn b(&self) -> u8 {
        self.bc[0]
    }

    pub fn set_b(&mut self, value: u8) {
        self.bc[0] = value;
    }

    pub fn c(&self) -> u8 {
        self.bc[1]
    }

    pub fn set_c(&mut self, value: u8) {
        self.bc[1] = value;
    }

    pub fn d(&self) -> u8 {
        self.de[0]
    }

    pub fn set_d(&mut self, value: u8) {
        self.de[0] = value;
    }

    pub fn e(&self) -> u8 {
        self.de[1]
    }

    pub fn set_e(&mut self, value: u8) {
        self.de[1] = value;
    }

    pub fn h(&self) -> u8 {
        self.hl[0]
    }

    pub fn set_h(&mut self, value: u8) {
        self.hl[0] = value;
    }

    pub fn l(&self) -> u8 {
        self.hl[1]
    }

    pub fn set_l(&mut self, value: u8) {
        self.hl[1] = value;
    }

    pub fn bc(&self) -> u16 {
        u16::from_be_bytes(self.bc)
    }

    pub fn set_bc(&mut self, value: u16) {
        self.bc = value.to_be_bytes();
    }

    pub fn de(&self) -> u16 {
        u16::from_be_bytes(self.de)
    }

    pub fn set_de(&mut self, value: u16) {
        self.de = value.to_be_bytes();
    }

    pub fn hl(&self) -> u16 {
        u16::from_be_bytes(self.hl)
    }

    pub fn set_hl(&mut self, value: u16) {
        self.hl = value.to_be_bytes();
    }

    pub fn af(&self) -> u16 {
        let flag_byte = ((self.carry_flag as u16) << 4)
            | ((self.h_flag as u16) << 5)
            | ((self.n_flag as u16) << 6)
            | ((self.zero_flag as u16) << 7);
        ((self.a as u16) << 8) | flag_byte
    }

    pub fn set_af(&mut self, value: u16) {
        let [a, flag_byte] = value.to_be_bytes();

        self.a = a;
        self.zero_flag = (flag_byte & 0x80) != 0;
        self.n_flag = (flag_byte & 0x40) != 0;
        self.h_flag = (flag_byte & 0x20) != 0;
        self.carry_flag = (flag_byte & 0x10) != 0;
    }

    pub fn zero_flag(&self) -> bool {
        self.zero_flag
    }

    pub fn set_zero_flag(&mut self, value: bool) {
        self.zero_flag = value;
    }

    pub fn carry_flag(&self) -> bool {
        self.carry_flag
    }

    pub fn set_carry_flag(&mut self, value: bool) {
        self.carry_flag = value;
    }

    pub fn interrupts_enabled(&self) -> bool {
        self.interrupts_enabled
    }

    pub fn set_interrupts_enabled(&mut self, value: bool) {
        self.interrupts_enabled = value;
    }
}
