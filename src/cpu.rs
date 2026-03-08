use crate::emulator::{Emulator, Interrupt};

impl Emulator {
    /// Execute an instruction, returning the number of clock cycles taken by the instruction.
    pub fn execute_instruction(&mut self) {
        let opcode = self.read_opcode();
        DISPATCH_TABLE[opcode as usize](self, opcode);
    }

    fn execute_cb_instruction(&mut self) {
        let opcode = self.read_opcode();
        CB_DISPATCH_TABLE[opcode as usize](self, opcode);
    }

    pub fn call_interrupt_handler(&mut self, interrupt: Interrupt) {
        // Treat as a regular call instruction to the interrupt handler address
        self.push_u16_to_stack(self.regs().pc());
        self.regs_mut().set_pc(interrupt.handler_address());
    }

    /// Read the opcode at PC and advance PC to the following byte.
    fn read_opcode(&mut self) -> Opcode {
        let pc = self.regs().pc();
        let byte = self.read_address(pc);
        self.regs_mut().set_pc(pc + 1);
        byte
    }

    /// Read the 8-bit immediate value at PC and advance PC to the following byte.
    fn read_imm8_operand(&mut self) -> u8 {
        let pc = self.regs().pc();
        let byte = self.read_address(pc);
        self.regs_mut().set_pc(pc + 1);
        byte
    }

    /// Read the 16-bit immediate value at PC and advance PC to the following byte.
    fn read_imm16_operand(&mut self) -> u16 {
        let pc = self.regs().pc();
        let low = self.read_address(pc) as u16;
        let high = self.read_address(pc + 1) as u16;
        self.regs_mut().set_pc(pc + 2);
        (high << 8) | low
    }

    /// Sets the zero flag iff the provided value is zero.
    fn set_zero_flag_for_value(&mut self, value: u8) {
        self.regs_mut().set_zero_flag(value == 0);
    }

    /// Byte representation of the carry flag (1 if set, 0 if not).
    fn carry_flag_byte_value(&self) -> u8 {
        if self.regs().carry_flag() { 1 } else { 0 }
    }

    /// Get the value of the specified 8-bit register operand.
    ///
    /// `r8_operand` must be in the range 0-7.
    fn read_r8_operand_value(&self, r8_operand: R8Operand) -> u8 {
        match r8_operand {
            0 => self.regs().b(),
            1 => self.regs().c(),
            2 => self.regs().d(),
            3 => self.regs().e(),
            4 => self.regs().h(),
            5 => self.regs().l(),
            R8_OPERAND_HL_MEM => self.read_address(self.regs().hl()),
            R8_OPERAND_A => self.regs().a(),
            _ => unreachable!("Invalid r8 operand"),
        }
    }

    /// Write a value to the specified 8-bit register operand.
    ///
    /// `r8_operand` must be in the range 0-7.
    fn write_r8_operand_value(&mut self, r8_operand: R8Operand, value: u8) {
        match r8_operand {
            0 => self.regs_mut().set_b(value),
            1 => self.regs_mut().set_c(value),
            2 => self.regs_mut().set_d(value),
            3 => self.regs_mut().set_e(value),
            4 => self.regs_mut().set_h(value),
            5 => self.regs_mut().set_l(value),
            R8_OPERAND_HL_MEM => self.write_address(self.regs().hl(), value),
            R8_OPERAND_A => self.regs_mut().set_a(value),
            _ => unreachable!("Invalid r8 operand"),
        }
    }

    /// Get the value of the specified 16-bit register operand.
    ///
    /// `r16_operand` must be in the range 0-3.
    fn read_r16_operand_value(&self, r16_operand: R16Operand) -> u16 {
        match r16_operand {
            0 => self.regs().bc(),
            1 => self.regs().de(),
            R16_OPERAND_HL => self.regs().hl(),
            R16_OPERAND_SP => self.regs().sp(),
            _ => unreachable!("Invalid r16 operand"),
        }
    }

    /// Write a value to the specified 16-bit register operand.
    ///
    /// `r16_operand` must be in the range 0-3.
    fn write_r16_operand_value(&mut self, r16_operand: R16Operand, value: u16) {
        match r16_operand {
            0 => self.regs_mut().set_bc(value),
            1 => self.regs_mut().set_de(value),
            R16_OPERAND_HL => self.regs_mut().set_hl(value),
            R16_OPERAND_SP => self.regs_mut().set_sp(value),
            _ => unreachable!("Invalid r16 operand"),
        }
    }

    /// Pop a 16-bit value from the stack. Value is stored in little endian order.
    fn pop_u16_from_stack(&mut self) -> u16 {
        let sp = self.regs().sp();

        let low = self.read_address(sp) as u16;
        let high = self.read_address(sp.wrapping_add(1)) as u16;
        let result = (high << 8) | low;

        let new_sp = sp.wrapping_add(2);
        self.regs_mut().set_sp(new_sp);

        result
    }

    /// Push a 16-bit value onto the stack. Value is stored in little endian order.
    fn push_u16_to_stack(&mut self, value: u16) {
        let sp = self.regs().sp();
        let [low, high] = value.to_le_bytes();

        let new_sp = sp.wrapping_sub(2);

        self.write_address(new_sp.wrapping_add(1), high);
        self.write_address(new_sp, low);

        self.regs_mut().set_sp(new_sp);
    }

    fn is_cc_met(&self, cc: CcOperand) -> bool {
        match cc {
            0 => !self.regs().zero_flag(),
            1 => self.regs().zero_flag(),
            2 => !self.regs().carry_flag(),
            3 => self.regs().carry_flag(),
            _ => unreachable!("Invalid condition code operand"),
        }
    }
}

struct InstructionFormatter {
    builder: String,
}

impl InstructionFormatter {
    fn new() -> Self {
        Self {
            builder: String::new(),
        }
    }

    fn finish(self) -> String {
        self.builder
    }

    #[allow(unused)]
    fn format(instrs: &[u8]) -> String {
        let format_handler = INSTRUCTION_FORMATTERS[instrs[0] as usize];
        let mut formatter = InstructionFormatter::new();
        format_handler(instrs, &mut formatter);

        formatter.finish()
    }

    fn opcode(&mut self, opcode: &str) {
        self.builder.push_str(opcode);
        self.builder.push(' ');
    }

    fn simple_opcode(&mut self, opcode: &str) {
        self.builder.push_str(opcode);
    }

    fn comma(&mut self) {
        self.builder.push_str(", ");
    }

    fn plus(&mut self) {
        self.builder.push_str(" + ");
    }

    fn r8_operand(&mut self, operand: R8Operand) {
        let formatted = match operand {
            0 => 'b',
            1 => 'c',
            2 => 'd',
            3 => 'e',
            4 => 'h',
            5 => 'l',
            R8_OPERAND_HL_MEM => {
                self.builder.push_str("[hl]");
                return;
            }
            R8_OPERAND_A => 'a',
            _ => panic!("Invalid r8 operand"),
        };

        self.builder.push(formatted);
    }

    fn a_operand(&mut self) {
        self.r8_operand(R8_OPERAND_A);
    }

    fn r16_operand(&mut self, operand: R16Operand) {
        let formatted = match operand {
            0 => "bc",
            1 => "de",
            R16_OPERAND_HL => "hl",
            R16_OPERAND_SP => "sp",
            _ => panic!("Invalid r16 operand"),
        };

        self.builder.push_str(formatted);
    }

    fn hl_operand(&mut self) {
        self.r16_operand(R16_OPERAND_HL);
    }

    fn sp_operand(&mut self) {
        self.r16_operand(R16_OPERAND_SP);
    }

    fn af_operand(&mut self) {
        self.builder.push_str("af");
    }

    fn r16_mem_operand(&mut self, operand: R16Operand) {
        let formatted = match operand {
            0 => "[bc]",
            1 => "[de]",
            _ => panic!("Invalid r16 mem operand"),
        };

        self.builder.push_str(formatted);
    }

    fn cc_operand(&mut self, operand: CcOperand) {
        let formatted = match operand {
            0 => "nz",
            1 => "z",
            2 => "nc",
            3 => "c",
            _ => panic!("Invalid cc operand"),
        };

        self.builder.push_str(formatted);
    }

    fn imm8_operand(&mut self, operand: u8) {
        self.builder.push_str(&format!("#{}", operand));
    }

    fn imm8_mem_operand(&mut self, operand: u8) {
        self.builder.push_str(&format!("[#{}]", operand));
    }

    fn imm16_operand(&mut self, operand: u16) {
        self.builder.push_str(&format!("#{}", operand));
    }

    fn imm16_mem_operand(&mut self, operand: u16) {
        self.builder.push_str(&format!("[#{}]", operand));
    }

    fn imm8_signed_operand(&mut self, operand: u8) {
        self.builder.push_str(&format!("#{}", operand as i8));
    }

    fn cmem_operand(&mut self) {
        self.builder.push_str("[c]");
    }

    fn hli_operand(&mut self) {
        self.builder.push_str("[hl+]");
    }

    fn hld_operand(&mut self) {
        self.builder.push_str("[hl-]");
    }

    fn rst_target_operand(&mut self, operand: u8) {
        self.builder.push_str(&format!("0x{:02X}", operand))
    }

    fn bit_index_operand(&mut self, operand: u8) {
        self.builder.push_str(&format!("{}", operand));
    }
}

type Opcode = u8;
type R8Operand = u8;
type R16Operand = u8;
type CcOperand = u8;
type InstructionHandler = fn(&mut Emulator, Opcode);
type FormatterHandler = fn(&[u8], &mut InstructionFormatter);

/// An r8 operand encoded in bits 0-2 of the opcode.
fn low_r8_operand(opcode: Opcode) -> R8Operand {
    opcode & 0x07
}

/// An r8 operand encoded in bits 3-5 of the opcode.
fn high_r8_operand(opcode: Opcode) -> R8Operand {
    (opcode >> 3) & 0x07
}

/// All possible r16 operands are encoded in bits 4-5 of the opcode.
fn r16_operand(opcode: Opcode) -> R16Operand {
    (opcode >> 4) & 0x03
}

/// A condition code operand encoded in bits 3-4 of the opcode.
fn cc_operand(opcode: Opcode) -> CcOperand {
    (opcode >> 3) & 0x03
}

/// A bit index operand encoded in bits 3-5 of the opcode.
fn bit_index_operand(opcode: Opcode) -> u8 {
    (opcode >> 3) & 0x07
}

/// Read an imm16 operand from the start of an instruction slice.
fn imm16_operand_from_slice(instrs: &[u8]) -> u16 {
    let low = instrs[0] as u16;
    let high = instrs[1] as u16;

    (high << 8) | low
}

const R8_OPERAND_HL_MEM: R8Operand = 6;
const R8_OPERAND_A: R8Operand = 7;

const R16_OPERAND_HL: R16Operand = 2;
const R16_OPERAND_SP: R16Operand = 3;

fn single_r8_operand_cycles(r8_operand: R8Operand) -> usize {
    if r8_operand == R8_OPERAND_HL_MEM {
        4
    } else {
        0
    }
}

fn double_r8_operand_cycles(r8_operand: R8Operand) -> usize {
    if r8_operand == R8_OPERAND_HL_MEM {
        8
    } else {
        0
    }
}

fn ldh_address(imm8_operand: u8) -> u16 {
    0xFF00 | (imm8_operand as u16)
}

/// Value of the half carry bit for an addition of two bytes
fn half_carry_for_add2(a: u8, b: u8) -> bool {
    (a & 0x0F) + (b & 0x0F) > 0x0F
}

/// Value of the half carry bit for an addition of three bytes
fn half_carry_for_add3(a: u8, b: u8, c: u8) -> bool {
    (a & 0x0F) + (b & 0x0F) + (c & 0x0F) > 0x0F
}

/// Value of the half carry bit for a subtraction of two bytes
fn half_carry_for_sub2(a: u8, b: u8) -> bool {
    (a & 0x0F) < (b & 0x0F)
}

/// Value of the half carry bit for a subtraction of three bytes
fn half_carry_for_sub3(a: u8, b: u8, c: u8) -> bool {
    (a & 0x0F) < (b & 0x0F) + (c & 0x0F)
}

/// Value of the half carry bit for an addition of two u16 values. Half carry checks the middle of
/// the upper byte.
fn half_carry_for_add2_u16(a: u16, b: u16) -> bool {
    (a & 0x0FFF) + (b & 0x0FFF) > 0x0FFF
}

/// Value of the half carry bit for the addition of an unsigned 16-bit and a signed 8-bit value.
/// Half carry checks for overflow from the lower nibble.
fn half_carry_for_sp_i8_add(a: u16, b: i8) -> bool {
    half_carry_for_add2(a as u8, b as u8)
}

/// Value of the carry bit for the addition of an unsigned 16-bit and a signed 8-bit value.
/// Carry checks for overflow from the lower byte.
fn carry_for_sp_i8_add(a: u16, b: i8) -> bool {
    let (_, overflowed) = (a as u8).overflowing_add(b as u8);
    overflowed
}

macro_rules! define_instruction {
    (
        $name:ident,
        fn execute($exec_emulator:pat, $exec_opcode:pat) $exec_body:block,
        fn format($fmt_instrs:pat, $fmt_formatter:pat) $fmt_body:block,
    ) => {
        #[allow(non_camel_case_types)]
        struct $name;

        impl $name {
            fn execute($exec_emulator: &mut Emulator, $exec_opcode: Opcode) {
                $exec_body
            }

            fn format($fmt_instrs: &[u8], $fmt_formatter: &mut InstructionFormatter) {
                $fmt_body
            }
        }
    };
}

define_instruction!(
    nop,
    fn execute(emulator, _) {
        // Do nothing
        emulator.schedule_next_instruction(4);
    },
    fn format(_, formatter) {
        formatter.simple_opcode("nop");
    },
);

fn has_tests_passed_registers(emulator: &Emulator) -> bool {
    emulator.regs().b() == 3
        && emulator.regs().c() == 5
        && emulator.regs().d() == 8
        && emulator.regs().e() == 13
        && emulator.regs().h() == 21
        && emulator.regs().l() == 34
}

fn has_tests_failed_registers(emulator: &Emulator) -> bool {
    emulator.regs().b() == 0x42
        && emulator.regs().c() == 0x42
        && emulator.regs().d() == 0x42
        && emulator.regs().e() == 0x42
        && emulator.regs().h() == 0x42
        && emulator.regs().l() == 0x42
}

fn check_test_results(emulator: &Emulator) {
    if has_tests_passed_registers(emulator) {
        println!("Test passed!");
    } else if has_tests_failed_registers(emulator) {
        println!("Test failed!",);
    }
}

define_instruction!(ld_r8_r8,
    fn execute(emulator, opcode) {
        let source_r8_operand = low_r8_operand(opcode);
        let dest_r8_operand = high_r8_operand(opcode);

        // Special case to check for test success or failure
        if emulator.in_test_mode() && source_r8_operand == 0 && dest_r8_operand == 0 {
            check_test_results(emulator);
        }

        let source_r8_value = emulator.read_r8_operand_value(source_r8_operand);
        emulator.write_r8_operand_value(dest_r8_operand, source_r8_value);

        let num_ticks = 4
            + single_r8_operand_cycles(source_r8_operand)
            + single_r8_operand_cycles(dest_r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let source_r8_operand = low_r8_operand(instrs[0]);
        let dest_r8_operand = high_r8_operand(instrs[0]);

        formatter.opcode("ld");
        formatter.r8_operand(dest_r8_operand);
        formatter.comma();
        formatter.r8_operand(source_r8_operand);
    },
);

define_instruction!(
    ld_r8_imm8,
    fn execute(emulator, opcode) {
        let r8_operand = high_r8_operand(opcode);
        let imm8_value = emulator.read_imm8_operand();

        emulator.write_r8_operand_value(r8_operand, imm8_value);

        let num_ticks = 8 + single_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let r8_operand = high_r8_operand(instrs[0]);
        let imm8_operand = instrs[1];

        formatter.opcode("ld");
        formatter.r8_operand(r8_operand);
        formatter.comma();
        formatter.imm8_operand(imm8_operand);
    },
);

define_instruction!(
    ld_r16_imm16,
    fn execute(emulator, opcode) {
        let r16_operand = r16_operand(opcode);
        let imm16_value = emulator.read_imm16_operand();

        emulator.write_r16_operand_value(r16_operand, imm16_value);

        emulator.schedule_next_instruction(12);
    },
    fn format(instrs, formatter) {
        let r16_operand = r16_operand(instrs[0]);
        let imm16_operand = imm16_operand_from_slice(&instrs[1..]);

        formatter.opcode("ld");
        formatter.r16_operand(r16_operand);
        formatter.comma();
        formatter.imm16_operand(imm16_operand);
    },
);

define_instruction!(
    ld_r16mem_a,
    fn execute(emulator, opcode) {
        let r16_operand = r16_operand(opcode);
        let r16_value = emulator.read_r16_operand_value(r16_operand);

        let accumulator = emulator.regs().a();
        emulator.write_address(r16_value, accumulator);

        emulator.schedule_next_instruction(8);
    },
    fn format(instrs, formatter) {
        let r16_operand = r16_operand(instrs[0]);

        formatter.opcode("ld");
        formatter.r16_mem_operand(r16_operand);
        formatter.comma();
        formatter.a_operand();
    },
);

define_instruction!(
    ld_a_r16mem,
    fn execute(emulator, opcode) {
        let r16_operand = r16_operand(opcode);
        let r16_value = emulator.read_r16_operand_value(r16_operand);

        let r16_mem = emulator.read_address(r16_value);
        emulator.regs_mut().set_a(r16_mem);

        emulator.schedule_next_instruction(8);
    },
    fn format(instrs, formatter) {
        let r16_operand = r16_operand(instrs[0]);

        formatter.opcode("ld");
        formatter.a_operand();
        formatter.comma();
        formatter.r16_mem_operand(r16_operand);
    },
);

define_instruction!(
    ld_imm16mem_a,
    fn execute(emulator, _) {
        let imm16 = emulator.read_imm16_operand();
        let accumulator = emulator.regs().a();

        emulator.write_address(imm16, accumulator);

        emulator.schedule_next_instruction(16);
    },
    fn format(instrs, formatter) {
        let imm16_operand = imm16_operand_from_slice(&instrs[1..]);

        formatter.opcode("ld");
        formatter.imm16_mem_operand(imm16_operand);
        formatter.comma();
        formatter.a_operand();
    },
);

define_instruction!(
    ld_a_imm16mem,
    fn execute(emulator, _) {
        let imm16 = emulator.read_imm16_operand();
        let imm16_mem = emulator.read_address(imm16);

        emulator.regs_mut().set_a(imm16_mem);

        emulator.schedule_next_instruction(16);
    },
    fn format(instrs, formatter) {
        let imm16_operand = imm16_operand_from_slice(&instrs[1..]);

        formatter.opcode("ld");
        formatter.a_operand();
        formatter.comma();
        formatter.imm16_mem_operand(imm16_operand);
    },
);

define_instruction!(
    ld_imm16mem_sp,
    fn execute (emulator, _) {
        let imm16 = emulator.read_imm16_operand();
        let [low, high] = emulator.regs().sp().to_le_bytes();

        emulator.write_address(imm16, low);
        emulator.write_address(imm16 + 1, high);

        emulator.schedule_next_instruction(20);
    },
    fn format(instrs, formatter) {
        let imm16_operand = imm16_operand_from_slice(&instrs[1..]);

        formatter.opcode("ld");
        formatter.imm16_mem_operand(imm16_operand);
        formatter.comma();
        formatter.sp_operand();
    },
);

define_instruction!(
    ldh_cmem_a,
    fn execute(emulator, _) {
        let accumulator = emulator.regs().a();
        let c = emulator.regs().c();

        emulator.write_address(ldh_address(c), accumulator);

        emulator.schedule_next_instruction(8);
    },
    fn format(_, formatter) {
        formatter.opcode("ldh");
        formatter.cmem_operand();
        formatter.comma();
        formatter.a_operand();
    },
);

define_instruction!(
    ldh_a_cmem,
    fn execute(emulator, _) {
        let c = emulator.regs().c();
        let c_mem = emulator.read_address(ldh_address(c));

        emulator.regs_mut().set_a(c_mem);

        emulator.schedule_next_instruction(8);
    },
    fn format(_, formatter) {
        formatter.opcode("ldh");
        formatter.a_operand();
        formatter.comma();
        formatter.cmem_operand();
    },
);

define_instruction!(
    ldh_imm8mem_a,
    fn execute(emulator, _) {
        let imm8 = emulator.read_imm8_operand();
        let accumulator = emulator.regs().a();

        emulator.write_address(ldh_address(imm8), accumulator);

        emulator.schedule_next_instruction(12);
    },
    fn format(instrs, formatter) {
        let imm8_operand = instrs[1];

        formatter.opcode("ldh");
        formatter.imm8_mem_operand(imm8_operand);
        formatter.comma();
        formatter.a_operand();
    },
);

define_instruction!(
    ldh_a_imm8mem,
    fn execute(emulator, _) {
        let imm8 = emulator.read_imm8_operand();
        let imm8_mem = emulator.read_address(ldh_address(imm8));

        emulator.regs_mut().set_a(imm8_mem);

        emulator.schedule_next_instruction(12);
    },
    fn format(instrs, formatter) {
        let imm8_operand = instrs[1];

        formatter.opcode("ldh");
        formatter.a_operand();
        formatter.comma();
        formatter.imm8_mem_operand(imm8_operand);
    },
);

define_instruction!(
    ld_hl_sp_imm8,
    fn execute (emulator, _) {
        let imm8_value = emulator.read_imm8_operand() as i8 as i16;
        let sp = emulator.regs().sp();

        let result = sp.wrapping_add_signed(imm8_value);
        emulator.regs_mut().set_hl(result);

        emulator.regs_mut().set_zero_flag(false);
        emulator.regs_mut().set_subtraction_flag(false);
        emulator.regs_mut().set_carry_flag(carry_for_sp_i8_add(sp, imm8_value as i8));
        emulator.regs_mut().set_half_carry_flag(half_carry_for_sp_i8_add(sp, imm8_value as i8));

        emulator.schedule_next_instruction(12);
    },
    fn format(instrs, formatter) {
        let imm8_operand = instrs[1];

        formatter.opcode("ld");
        formatter.hl_operand();
        formatter.comma();
        formatter.sp_operand();
        formatter.plus();
        formatter.imm8_signed_operand(imm8_operand);
    },
);

define_instruction!(
    ld_a_hli,
    fn execute (emulator, _) {
        let hl = emulator.regs().hl();
        let hl_mem = emulator.read_address(hl);

        emulator.regs_mut().set_a(hl_mem);
        emulator.regs_mut().set_hl(hl.wrapping_add(1));

        emulator.schedule_next_instruction(8);
    },
    fn format(_, formatter) {
        formatter.opcode("ld");
        formatter.a_operand();
        formatter.comma();
        formatter.hli_operand();
    },
);

define_instruction!(
    ld_a_hld,
    fn execute (emulator, _) {
        let hl = emulator.regs().hl();
        let hl_mem = emulator.read_address(hl);

        emulator.regs_mut().set_a(hl_mem);
        emulator.regs_mut().set_hl(hl.wrapping_sub(1));

        emulator.schedule_next_instruction(8);
    },
    fn format(_, formatter) {
        formatter.opcode("ld");
        formatter.a_operand();
        formatter.comma();
        formatter.hld_operand();
    },
);

define_instruction!(
    ld_hli_a,
    fn execute (emulator, _) {
        let hl = emulator.regs().hl();
        let accumulator = emulator.regs().a();

        emulator.write_address(hl, accumulator);
        emulator.regs_mut().set_hl(hl.wrapping_add(1));

        emulator.schedule_next_instruction(8);
    },
    fn format(_, formatter) {
        formatter.opcode("ld");
        formatter.hli_operand();
        formatter.comma();
        formatter.a_operand();
    },
);

define_instruction!(
    ld_hld_a,
    fn execute (emulator, _) {
        let hl = emulator.regs().hl();
        let accumulator = emulator.regs().a();

        emulator.write_address(hl, accumulator);
        emulator.regs_mut().set_hl(hl.wrapping_sub(1));

        emulator.schedule_next_instruction(8);
    },
    fn format(_, formatter) {
        formatter.opcode("ld");
        formatter.hld_operand();
        formatter.comma();
        formatter.a_operand();
    },
);

define_instruction!(
    ld_sp_hl,
    fn execute (emulator, _) {
        let hl = emulator.regs().hl();
        emulator.regs_mut().set_sp(hl);

        emulator.schedule_next_instruction(8);
    },
    fn format(_, formatter) {
        formatter.opcode("ld");
        formatter.sp_operand();
        formatter.comma();
        formatter.hl_operand();
    },
);

define_instruction!(
    inc_r16,
    fn execute (emulator, operand) {
        let r16_operand = r16_operand(operand);
        let r16_value = emulator.read_r16_operand_value(r16_operand);

        let result = r16_value.wrapping_add(1);
        emulator.write_r16_operand_value(r16_operand, result);

        emulator.schedule_next_instruction(8);
    },
    fn format(instrs, formatter) {
        let r16_operand = r16_operand(instrs[0]);

        formatter.opcode("inc");
        formatter.r16_operand(r16_operand);
    },
);

define_instruction!(
    dec_r16,
    fn execute (emulator, operand) {
        let r16_operand = r16_operand(operand);
        let r16_value = emulator.read_r16_operand_value(r16_operand);

        let result = r16_value.wrapping_sub(1);
        emulator.write_r16_operand_value(r16_operand, result);

        emulator.schedule_next_instruction(8);
    },
    fn format(instrs, formatter) {
        let r16_operand = r16_operand(instrs[0]);

        formatter.opcode("dec");
        formatter.r16_operand(r16_operand);
    },
);

define_instruction!(
    inc_r8,
    fn execute (emulator, opcode) {
        let r8_operand = high_r8_operand(opcode);
        let r8_value = emulator.read_r8_operand_value(r8_operand);

        let result = r8_value.wrapping_add(1);
        emulator.write_r8_operand_value(r8_operand, result);

        // Carry flag is not set
        emulator.set_zero_flag_for_value(result);
        emulator.regs_mut().set_subtraction_flag(false);
        emulator.regs_mut().set_half_carry_flag(half_carry_for_add2(r8_value, 1));

        let num_ticks = 4 + double_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let r8_operand = high_r8_operand(instrs[0]);

        formatter.opcode("inc");
        formatter.r8_operand(r8_operand);
    },
);

define_instruction!(
    dec_r8,
    fn execute (emulator, opcode) {
        let r8_operand = high_r8_operand(opcode);
        let r8_value = emulator.read_r8_operand_value(r8_operand);

        let result = r8_value.wrapping_sub(1);
        emulator.write_r8_operand_value(r8_operand, result);

        // Carry flag is not set
        emulator.set_zero_flag_for_value(result);
        emulator.regs_mut().set_subtraction_flag(true);
        emulator.regs_mut().set_half_carry_flag(half_carry_for_sub2(r8_value, 1));

        let num_ticks = 4 + double_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let r8_operand = high_r8_operand(instrs[0]);

        formatter.opcode("dec");
        formatter.r8_operand(r8_operand);
    },
);

define_instruction!(
    add_hl_r16,
    fn execute (emulator, opcode) {
        let r16_operand = r16_operand(opcode);
        let r16_value = emulator.read_r16_operand_value(r16_operand);
        let hl = emulator.regs().hl();

        let (result, carried) = hl.overflowing_add(r16_value);
        emulator.regs_mut().set_hl(result);

        // Zero flag is not set
        emulator.regs_mut().set_carry_flag(carried);
        emulator.regs_mut().set_subtraction_flag(false);
        emulator.regs_mut().set_half_carry_flag(half_carry_for_add2_u16(hl, r16_value));

        emulator.schedule_next_instruction(8);
    },
    fn format(instrs, formatter) {
        let r16_operand = r16_operand(instrs[0]);

        formatter.opcode("add");
        formatter.hl_operand();
        formatter.comma();
        formatter.r16_operand(r16_operand);
    },
);

define_instruction!(
    rlca,
    fn execute(emulator, _) {
        // Rotate register A left, setting carry flag based on bit that was rotated around.
        let acc = emulator.regs().a();
        let high_bit = acc & 0x80;

        let rotated_acc = (acc << 1) | (high_bit >> 7);
        emulator.regs_mut().set_a(rotated_acc);

        emulator.regs_mut().set_carry_flag(high_bit != 0);
        emulator.regs_mut().set_zero_flag(false);
        emulator.regs_mut().set_bcd_flags_zero();

        emulator.schedule_next_instruction(4);
    },
    fn format(_, formatter) {
        formatter.simple_opcode("rlca");
    },
);

define_instruction!(
    rrca,
    fn execute(emulator, _) {
        // Rotate register A right, setting carry flag based on bit that was rotated around.
        let acc = emulator.regs().a();
        let low_bit = acc & 0x01;

        let rotated_acc = (acc >> 1) | (low_bit << 7);
        emulator.regs_mut().set_a(rotated_acc);

        emulator.regs_mut().set_carry_flag(low_bit != 0);
        emulator.regs_mut().set_zero_flag(false);
        emulator.regs_mut().set_bcd_flags_zero();

        emulator.schedule_next_instruction(4);
    },
    fn format(_, formatter) {
        formatter.simple_opcode("rrca");
    },
);

define_instruction!(
    rla,
    fn execute(emulator, _) {
        // Rotate register A left through carry flag.
        let acc = emulator.regs().a();
        let carry_flag_byte = emulator.carry_flag_byte_value();

        let high_bit = acc & 0x80;
        let rotated_acc = (acc << 1) | carry_flag_byte;
        emulator.regs_mut().set_a(rotated_acc);

        emulator.regs_mut().set_carry_flag(high_bit != 0);
        emulator.regs_mut().set_zero_flag(false);
        emulator.regs_mut().set_bcd_flags_zero();

        emulator.schedule_next_instruction(4);
    },
    fn format(_, formatter) {
        formatter.simple_opcode("rla");
    },
);

define_instruction!(
    rra,
    fn execute(emulator, _) {
        // Rotate register A right through carry flag.
        let acc = emulator.regs().a();
        let carry_flag_byte = emulator.carry_flag_byte_value();

        let low_bit = acc & 0x01;
        let rotated_acc = (acc >> 1) | (carry_flag_byte << 7);
        emulator.regs_mut().set_a(rotated_acc);

        emulator.regs_mut().set_carry_flag(low_bit != 0);
        emulator.regs_mut().set_zero_flag(false);
        emulator.regs_mut().set_bcd_flags_zero();

        emulator.schedule_next_instruction(4);
    },
    fn format(_, formatter) {
        formatter.simple_opcode("rra");
    },
);

define_instruction!(
    daa,
    fn execute(emulator, _) {
        let acc = emulator.regs().a();
        let subtraction_flag = emulator.regs().subtraction_flag();
        let carry_flag = emulator.regs().carry_flag();
        let half_carry_flag = emulator.regs().half_carry_flag();

        let mut adjustment = 0x0;
        let mut carried = false;

        if half_carry_flag || (!subtraction_flag && (acc & 0x0F) > 0x09) {
            adjustment += 0x06;
        }

        if carry_flag || (!subtraction_flag && acc > 0x99) {
            adjustment += 0x60;
            carried = true;
        }

        let result = if subtraction_flag {
            acc.wrapping_sub(adjustment)
        } else {
            acc.wrapping_add(adjustment)
        };

        emulator.regs_mut().set_a(result);

        emulator.set_zero_flag_for_value(result);
        emulator.regs_mut().set_half_carry_flag(false);
        emulator.regs_mut().set_carry_flag(carried);

        emulator.schedule_next_instruction(4);
    },
    fn format(_, formatter) {
        formatter.simple_opcode("daa");
    },
);

define_instruction!(
    cpl,
    fn execute(emulator, _) {
        let accumulator = emulator.regs().a();
        emulator.regs_mut().set_a(!accumulator);

        emulator.regs_mut().set_subtraction_flag(true);
        emulator.regs_mut().set_half_carry_flag(true);

        emulator.schedule_next_instruction(4);
    },
    fn format(_, formatter) {
        formatter.simple_opcode("cpl");
    },
);

define_instruction!(
    scf,
    fn execute(emulator, _) {
        emulator.regs_mut().set_carry_flag(true);
        emulator.regs_mut().set_bcd_flags_zero();

        emulator.schedule_next_instruction(4);
    },
    fn format(_, formatter) {
        formatter.simple_opcode("scf");
    },
);

define_instruction!(
    ccf,
    fn execute (emulator, _) {
        let carry_flag = emulator.regs().carry_flag();

        emulator.regs_mut().set_carry_flag(!carry_flag);
        emulator.regs_mut().set_bcd_flags_zero();

        emulator.schedule_next_instruction(4);
    },
    fn format(_, formatter) {
        formatter.simple_opcode("ccf");
    },
);

define_instruction!(
    stop,
    fn execute(emulator, _) {
        let has_button_pressed = emulator.joypad_reg() & 0x0F != 0x0F;
        let is_speed_switch_armed = emulator.key1() & 0x01 == 1;
        let has_interrupts = emulator.interrupt_bits() != 0;
        let new_speed = !emulator.is_double_speed();

        if emulator.in_cgb_mode() && !has_button_pressed && is_speed_switch_armed {
            if !has_interrupts {
                // This STOP is a two-byte instruction where the second byte is ignored
                let _ = emulator.read_imm8_operand();

                emulator.set_is_double_speed(new_speed);
                emulator.start_speed_switch();
                emulator.reset_divider_register();
                emulator.write_key1(0x00);

                return;
            } else if emulator.regs().interrupts_enabled() {
                // STOP is a one-byte insruction that immediately enters double-speed mode
                emulator.set_is_double_speed(new_speed);
                emulator.reset_divider_register();
                emulator.write_key1(0x00);

                return;
            }
        }

        panic!("STOP instruction that does not result in a speed switch")
    },
    fn format(_, formatter) {
        formatter.simple_opcode("stop");
    },
);

define_instruction!(
    halt,
    fn execute(emulator, _) {
        // Note that if there are pending interrupts but the IME is disabled then the CPU does not halt.
        if emulator.regs().interrupts_enabled() || emulator.interrupt_bits() == 0 {
            emulator.halt_cpu();
        }

        emulator.schedule_next_instruction(4);
    },
    fn format(_, formatter) {
        formatter.simple_opcode("halt");
    },
);

/// Any arithmetic operation between the accumulator and an r8 operand.
fn arithmetic_a_r8_instruction(
    emulator: &mut Emulator,
    opcode: Opcode,
    operation: fn(&mut Emulator, u8, u8) -> u8,
) {
    let r8_operand = low_r8_operand(opcode);
    let r8_value = emulator.read_r8_operand_value(r8_operand);
    let acc = emulator.regs().a();

    let result = operation(emulator, acc, r8_value);

    emulator.regs_mut().set_a(result);
    emulator.set_zero_flag_for_value(result);

    let num_ticks = 4 + single_r8_operand_cycles(r8_operand);
    emulator.schedule_next_instruction(num_ticks);
}

fn format_arithmetic_a_r8_instruction(
    instrs: &[u8],
    formatter: &mut InstructionFormatter,
    opcode: &str,
) {
    let r8_operand = low_r8_operand(instrs[0]);

    formatter.opcode(opcode);
    formatter.a_operand();
    formatter.comma();
    formatter.r8_operand(r8_operand);
}

define_instruction!(
    add_a_r8,
    fn execute (emulator, opcode) {
        arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
            let (result, carried) = acc.overflowing_add(r8_value);

            emulator.regs_mut().set_carry_flag(carried);
            emulator.regs_mut().set_subtraction_flag(false);
            emulator.regs_mut().set_half_carry_flag(half_carry_for_add2(acc, r8_value));

            result
        });
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_r8_instruction(instrs, formatter, "add");
    },
);

define_instruction!(
    sub_a_r8,
    fn execute (emulator, opcode) {
        arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
            let (result, carried) = acc.overflowing_sub(r8_value);

            emulator.regs_mut().set_carry_flag(carried);
            emulator.regs_mut().set_subtraction_flag(true);
            emulator.regs_mut().set_half_carry_flag(half_carry_for_sub2(acc, r8_value));

            result
        });
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_r8_instruction(instrs, formatter, "sub");
    },
);

define_instruction!(
    adc_a_r8,
    fn execute (emulator, opcode) {
        arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
            let carry_byte = emulator.carry_flag_byte_value();

            let (tmp, carry1) = acc.overflowing_add(r8_value);
            let (result, carry2) = tmp.overflowing_add(carry_byte);

            emulator.regs_mut().set_carry_flag(carry1 || carry2);
            emulator.regs_mut().set_subtraction_flag(false);
            emulator.regs_mut().set_half_carry_flag(half_carry_for_add3(acc, r8_value, carry_byte));

            result
        });
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_r8_instruction(instrs, formatter, "adc");
    },
);

define_instruction!(
    sbc_a_r8,
    fn execute (emulator, opcode) {
        arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
            let carry_byte = emulator.carry_flag_byte_value();

            let (tmp, carry1) = acc.overflowing_sub(r8_value);
            let (result, carry2) = tmp.overflowing_sub(carry_byte);

            emulator.regs_mut().set_carry_flag(carry1 || carry2);
            emulator.regs_mut().set_subtraction_flag(true);
            emulator.regs_mut().set_half_carry_flag(half_carry_for_sub3(acc, r8_value, carry_byte));

            result
        });
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_r8_instruction(instrs, formatter, "sbc");
    },
);

define_instruction!(
    and_a_r8,
    fn execute (emulator, opcode) {
        arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
            emulator.regs_mut().set_carry_flag(false);
            emulator.regs_mut().set_subtraction_flag(false);
            emulator.regs_mut().set_half_carry_flag(true);

            acc & r8_value
        });
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_r8_instruction(instrs, formatter, "and");
    },
);

define_instruction!(
    xor_a_r8,
    fn execute (emulator, opcode) {
        arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
            emulator.regs_mut().set_carry_flag(false);
            emulator.regs_mut().set_bcd_flags_zero();

            acc ^ r8_value
        });
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_r8_instruction(instrs, formatter, "xor");
    },
);

define_instruction!(
    or_a_r8,
    fn execute (emulator, opcode) {
        arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
            emulator.regs_mut().set_carry_flag(false);
            emulator.regs_mut().set_bcd_flags_zero();

            acc | r8_value
        });
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_r8_instruction(instrs, formatter, "or");
    },
);

// Identical to sub_a_r8, but does not write the result to the accumulator.
define_instruction!(
    cp_a_r8,
    fn execute (emulator, opcode) {
        let r8_operand = low_r8_operand(opcode);
        let r8_value = emulator.read_r8_operand_value(r8_operand);
        let acc = emulator.regs().a();

        let (result, carried) = acc.overflowing_sub(r8_value);

        emulator.regs_mut().set_carry_flag(carried);
        emulator.set_zero_flag_for_value(result);
        emulator.regs_mut().set_subtraction_flag(true);
        emulator.regs_mut().set_half_carry_flag(half_carry_for_sub2(acc, r8_value));

        let num_ticks = 4 + single_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_r8_instruction(instrs, formatter, "cp");
    },
);

/// Any arithmetic operation between the accumulator and an imm8 operand.
fn arithmetic_a_imm8_instruction(
    emulator: &mut Emulator,
    operation: fn(&mut Emulator, u8, u8) -> u8,
) {
    let imm8_value = emulator.read_imm8_operand();
    let acc = emulator.regs().a();

    let result = operation(emulator, acc, imm8_value);

    emulator.regs_mut().set_a(result);
    emulator.set_zero_flag_for_value(result);

    emulator.schedule_next_instruction(8);
}

fn format_arithmetic_a_imm8_instruction(
    instrs: &[u8],
    formatter: &mut InstructionFormatter,
    opcode: &str,
) {
    let imm8_operand = instrs[1];

    formatter.opcode(opcode);
    formatter.a_operand();
    formatter.comma();
    formatter.imm8_operand(imm8_operand);
}

define_instruction!(
    add_a_imm8,
    fn execute (emulator, _) {
        arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
            let (result, carried) = acc.overflowing_add(imm8_value);

            emulator.regs_mut().set_carry_flag(carried);
            emulator.regs_mut().set_subtraction_flag(false);
            emulator.regs_mut().set_half_carry_flag(half_carry_for_add2(acc, imm8_value));

            result
        });
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_imm8_instruction(instrs, formatter, "add");
    },
);

define_instruction!(
    sub_a_imm8,
    fn execute (emulator, _) {
        arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
            let (result, carried) = acc.overflowing_sub(imm8_value);

            emulator.regs_mut().set_carry_flag(carried);
            emulator.regs_mut().set_subtraction_flag(true);
            emulator.regs_mut().set_half_carry_flag(half_carry_for_sub2(acc, imm8_value));

            result
        });
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_imm8_instruction(instrs, formatter, "sub");
    },
);

define_instruction!(
    adc_a_imm8,
    fn execute (emulator, _) {
        arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
            let carry_byte = emulator.carry_flag_byte_value();

            let (tmp, carry1) = acc.overflowing_add(imm8_value);
            let (result, carry2) = tmp.overflowing_add(carry_byte);

            emulator.regs_mut().set_carry_flag(carry1 || carry2);
            emulator.regs_mut().set_subtraction_flag(false);
            emulator.regs_mut().set_half_carry_flag(half_carry_for_add3(acc, imm8_value, carry_byte));

            result
        });
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_imm8_instruction(instrs, formatter, "adc");
    },
);

define_instruction!(
    sbc_a_imm8,
    fn execute (emulator, _) {
        arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
            let carry_byte = emulator.carry_flag_byte_value();

            let (tmp, carry1) = acc.overflowing_sub(imm8_value);
            let (result, carry2) = tmp.overflowing_sub(carry_byte);

            emulator.regs_mut().set_carry_flag(carry1 || carry2);
            emulator.regs_mut().set_subtraction_flag(true);
            emulator.regs_mut().set_half_carry_flag(half_carry_for_sub3(acc, imm8_value, carry_byte));

            result
        });
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_imm8_instruction(instrs, formatter, "sbc");
    },
);

define_instruction!(
    and_a_imm8,
    fn execute (emulator, _) {
        arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
            emulator.regs_mut().set_carry_flag(false);
            emulator.regs_mut().set_subtraction_flag(false);
            emulator.regs_mut().set_half_carry_flag(true);

            acc & imm8_value
        });
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_imm8_instruction(instrs, formatter, "and");
    },
);

define_instruction!(
    xor_a_imm8,
    fn execute (emulator, _) {
        arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
            emulator.regs_mut().set_carry_flag(false);
            emulator.regs_mut().set_bcd_flags_zero();

            acc ^ imm8_value
        });
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_imm8_instruction(instrs, formatter, "xor");
    },
);

define_instruction!(
    or_a_imm8,
    fn execute (emulator, _) {
        arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
            emulator.regs_mut().set_carry_flag(false);
            emulator.regs_mut().set_bcd_flags_zero();

            acc | imm8_value
        });
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_imm8_instruction(instrs, formatter, "or");
    },
);

// Identical to sub_a_imm8, but does not write the result to the accumulator.
define_instruction!(
    cp_a_imm8,
    fn execute (emulator, _) {
        let imm8_value = emulator.read_imm8_operand();
        let acc = emulator.regs().a();

        let (result, carried) = acc.overflowing_sub(imm8_value);

        emulator.regs_mut().set_carry_flag(carried);
        emulator.set_zero_flag_for_value(result);
        emulator.regs_mut().set_subtraction_flag(true);
        emulator.regs_mut().set_half_carry_flag(half_carry_for_sub2(acc, imm8_value));

        emulator.schedule_next_instruction(8);
    },
    fn format(instrs, formatter) {
        format_arithmetic_a_imm8_instruction(instrs, formatter, "cp");
    },
);

define_instruction!(
    jr_imm8,
    fn execute(emulator, _) {
        let signed_offset = emulator.read_imm8_operand() as i8 as i16;
        let pc = emulator.regs().pc();

        emulator.regs_mut().set_pc(pc.wrapping_add_signed(signed_offset));

        emulator.schedule_next_instruction(12);
    },
    fn format(instrs, formatter) {
        let imm8_operand = instrs[1];

        formatter.opcode("jr");
        formatter.imm8_signed_operand(imm8_operand);
    },
);

define_instruction!(
    jr_cc_imm8,
    fn execute(emulator, opcode) {
        let signed_offset = emulator.read_imm8_operand() as i8 as i16;
        let pc = emulator.regs().pc();

        if !emulator.is_cc_met(cc_operand(opcode)) {
            emulator.schedule_next_instruction(8);
            return;
        }

        emulator.regs_mut().set_pc(pc.wrapping_add_signed(signed_offset));

        emulator.schedule_next_instruction(12);
    },
    fn format(instrs, formatter) {
        let cc_operand = cc_operand(instrs[0]);
        let imm8_operand = instrs[1];

        formatter.opcode("jr");
        formatter.cc_operand(cc_operand);
        formatter.comma();
        formatter.imm8_signed_operand(imm8_operand);
    },
);

define_instruction!(
    jp_imm16,
    fn execute (emulator, _) {
        let imm16 = emulator.read_imm16_operand();
        emulator.regs_mut().set_pc(imm16);

        emulator.schedule_next_instruction(16);
    },
    fn format(instrs, formatter) {
        let imm16_operand = imm16_operand_from_slice(&instrs[1..]);

        formatter.opcode("jp");
        formatter.imm16_operand(imm16_operand);
    },
);

define_instruction!(
    jp_cond_imm16,
    fn execute (emulator, opcode) {
        let imm16 = emulator.read_imm16_operand();
        let cc = cc_operand(opcode);

        if !emulator.is_cc_met(cc) {
            emulator.schedule_next_instruction(12);
            return;
        }

        emulator.regs_mut().set_pc(imm16);

        emulator.schedule_next_instruction(16);
    },
    fn format(instrs, formatter) {
        let cc_operand = cc_operand(instrs[0]);
        let imm16_operand = imm16_operand_from_slice(&instrs[1..]);

        formatter.opcode("jp");
        formatter.cc_operand(cc_operand);
        formatter.comma();
        formatter.imm16_operand(imm16_operand);
    },
);

define_instruction!(
    jp_hl,
    fn execute (emulator, _) {
        let hl = emulator.regs().hl();
        emulator.regs_mut().set_pc(hl);

        emulator.schedule_next_instruction(4);
    },
    fn format(_, formatter) {
       formatter.opcode("jp");
       formatter.hl_operand();
    },
);

define_instruction!(
    ret,
    fn execute (emulator, _) {
        let saved_pc = emulator.pop_u16_from_stack();
        emulator.regs_mut().set_pc(saved_pc);

        emulator.schedule_next_instruction(16);
    },
    fn format(_, formatter) {
        formatter.simple_opcode("ret");
    },
);

define_instruction!(
    ret_cc,
    fn execute (emulator, opcode) {
        let cc = cc_operand(opcode);

        if !emulator.is_cc_met(cc) {
            emulator.schedule_next_instruction(8);
            return;
        }

        let saved_pc = emulator.pop_u16_from_stack();
        emulator.regs_mut().set_pc(saved_pc);

        emulator.schedule_next_instruction(20);
    },
    fn format(instrs, formatter) {
        let cc_operand = cc_operand(instrs[0]);

        formatter.opcode("ret");
        formatter.cc_operand(cc_operand);
    },
);

define_instruction!(
    reti,
    fn execute (emulator, _) {
        let saved_pc = emulator.pop_u16_from_stack();
        emulator.regs_mut().set_pc(saved_pc);

        // Should immediately enable interrupts after returning
        emulator.regs_mut().set_interrupts_enabled(true);

        emulator.schedule_next_instruction(16);
    },
    fn format(_, formatter) {
        formatter.simple_opcode("reti");
    },
);

define_instruction!(
    call_imm16,
    fn execute(emulator, _) {
        let imm16 = emulator.read_imm16_operand();

        // Push the current PC to the stack then set PC to the operand
        emulator.push_u16_to_stack(emulator.regs().pc());
        emulator.regs_mut().set_pc(imm16);

        emulator.schedule_next_instruction(24);
    },
    fn format(instrs, formatter) {
        let imm16_operand = imm16_operand_from_slice(&instrs[1..]);

        formatter.opcode("call");
        formatter.imm16_operand(imm16_operand);
    },
);

define_instruction!(
    call_cc_imm16,
    fn execute(emulator, opcode) {
        let imm16 = emulator.read_imm16_operand();
        let cc = cc_operand(opcode);

        if !emulator.is_cc_met(cc) {
            emulator.schedule_next_instruction(12);
            return;
        }

        // Push the current PC to the stack then set PC to the operand
        emulator.push_u16_to_stack(emulator.regs().pc());
        emulator.regs_mut().set_pc(imm16);

        emulator.schedule_next_instruction(24);
    },
    fn format(instrs, formatter) {
        let cc_operand = cc_operand(instrs[0]);
        let imm16_operand = imm16_operand_from_slice(&instrs[1..]);

        formatter.opcode("call");
        formatter.cc_operand(cc_operand);
        formatter.comma();
        formatter.imm16_operand(imm16_operand);
    },
);

define_instruction!(
    rst_tgt,
    fn execute(emulator, opcode) {
        let target_address = opcode & 0x38;

        // Push the current PC to the stack then set PC to the target address
        emulator.push_u16_to_stack(emulator.regs().pc());
        emulator.regs_mut().set_pc(target_address as u16);

        emulator.schedule_next_instruction(16);
    },
    fn format(instrs, formatter) {
        let target_address = instrs[0] & 0x38;

        formatter.opcode("rst");
        formatter.rst_target_operand(target_address);
    },
);

define_instruction!(
    pop_r16,
    fn execute(emulator, opcode) {
        let r16_operand = r16_operand(opcode);
        let popped_value = emulator.pop_u16_from_stack();

        emulator.write_r16_operand_value(r16_operand, popped_value);

        emulator.schedule_next_instruction(12);
    },
    fn format(instrs, formatter) {
        let r16_operand = r16_operand(instrs[0]);

        formatter.opcode("pop");
        formatter.r16_operand(r16_operand);
    },
);

define_instruction!(
    pop_af,
    fn execute(emulator, _) {
        let popped_value = emulator.pop_u16_from_stack();

        emulator.regs_mut().set_af(popped_value);

        emulator.schedule_next_instruction(12);
    },
    fn format(_, formatter) {
        formatter.opcode("pop");
        formatter.af_operand();
    },
);

define_instruction!(
    push_r16,
    fn execute(emulator, opcode) {
        let r16_operand = r16_operand(opcode);
        let r16_value = emulator.read_r16_operand_value(r16_operand);

        emulator.push_u16_to_stack(r16_value);

        emulator.schedule_next_instruction(16);
    },
    fn format(instrs, formatter) {
        let r16_operand = r16_operand(instrs[0]);

        formatter.opcode("push");
        formatter.r16_operand(r16_operand);
    },
);

define_instruction!(
    push_af,
    fn execute(emulator, _) {
        let af = emulator.regs().af();
        emulator.push_u16_to_stack(af);

        emulator.schedule_next_instruction(16);
    },
    fn format(_, formatter) {
        formatter.opcode("push");
        formatter.af_operand();
    },
);

define_instruction!(
    add_sp_imm8,
    fn execute (emulator, _) {
        let signed_operand = emulator.read_imm8_operand() as i8 as i16;
        let sp = emulator.regs().sp();

        let result = sp.wrapping_add_signed(signed_operand);
        emulator.regs_mut().set_sp(result);

        emulator.regs_mut().set_zero_flag(false);
        emulator.regs_mut().set_subtraction_flag(false);
        emulator.regs_mut().set_carry_flag(carry_for_sp_i8_add(sp, signed_operand as i8));
        emulator.regs_mut().set_half_carry_flag(half_carry_for_sp_i8_add(sp, signed_operand as i8));

        emulator.schedule_next_instruction(16);
    },
    fn format(instrs, formatter) {
        let imm8_operand = instrs[1];

        formatter.opcode("add");
        formatter.sp_operand();
        formatter.comma();
        formatter.imm8_signed_operand(imm8_operand);
    },
);

define_instruction!(
    di,
    fn execute (emulator, _) {
        emulator.regs_mut().set_interrupts_enabled(false);

        emulator.schedule_next_instruction(4);
    },
    fn format(_, formatter) {
        formatter.opcode("di");
    },
);

define_instruction!(
    ei,
    fn execute(emulator, _) {
        // Enable interrupts after the next instruction, so set a pending flag
        emulator.add_pending_enable_interrupts();

        emulator.schedule_next_instruction(4);
    },
    fn format(_, formatter) {
        formatter.opcode("ei");
    },
);

define_instruction!(
    cb_prefix,
    fn execute (emulator, _) {
        emulator.execute_cb_instruction();
    },
    fn format(instrs, formatter) {
        let format_handler = CB_FORMATTER_TABLE[instrs[1] as usize];
        format_handler(&instrs[1..], formatter);
    },
);

define_instruction!(
    rlc,
    fn execute(emulator, opcode) {
        // Rotate register left, setting carry flag based on bit that was rotated around.
        let r8_operand = low_r8_operand(opcode);
        let r8_value = emulator.read_r8_operand_value(r8_operand);
        let high_bit = r8_value & 0x80;

        let rotated_reg = (r8_value << 1) | (high_bit >> 7);
        emulator.write_r8_operand_value(r8_operand, rotated_reg);

        emulator.regs_mut().set_carry_flag(high_bit != 0);
        emulator.set_zero_flag_for_value(rotated_reg);
        emulator.regs_mut().set_bcd_flags_zero();

        let num_ticks = 8 + double_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let r8_operand = low_r8_operand(instrs[0]);

        formatter.opcode("rlc");
        formatter.r8_operand(r8_operand);
    },
);

define_instruction!(
    rrc,
    fn execute(emulator, opcode) {
        // Rotate register right, setting carry flag based on bit that was rotated around.
        let r8_operand = low_r8_operand(opcode);
        let r8_value = emulator.read_r8_operand_value(r8_operand);
        let low_bit = r8_value & 0x01;

        let rotated_reg = (r8_value >> 1) | (low_bit << 7);
        emulator.write_r8_operand_value(r8_operand, rotated_reg);

        emulator.regs_mut().set_carry_flag(low_bit != 0);
        emulator.set_zero_flag_for_value(rotated_reg);
        emulator.regs_mut().set_bcd_flags_zero();

        let num_ticks = 8 + double_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let r8_operand = low_r8_operand(instrs[0]);

        formatter.opcode("rrc");
        formatter.r8_operand(r8_operand);
    },
);

define_instruction!(
    rl,
    fn execute(emulator, opcode) {
        // Rotate register left through carry flag.
        let r8_operand = low_r8_operand(opcode);
        let r8_value = emulator.read_r8_operand_value(r8_operand);
        let carry_flag_byte = emulator.carry_flag_byte_value();

        let high_bit = r8_value & 0x80;
        let rotated_reg = (r8_value << 1) | carry_flag_byte;
        emulator.write_r8_operand_value(r8_operand, rotated_reg);

        emulator.regs_mut().set_carry_flag(high_bit != 0);
        emulator.set_zero_flag_for_value(rotated_reg);
        emulator.regs_mut().set_bcd_flags_zero();

        let num_ticks = 8 + double_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let r8_operand = low_r8_operand(instrs[0]);

        formatter.opcode("rl");
        formatter.r8_operand(r8_operand);
    },
);

define_instruction!(
    rr,
    fn execute(emulator, opcode) {
        // Rotate register right through carry flag.
        let r8_operand = low_r8_operand(opcode);
        let r8_value = emulator.read_r8_operand_value(r8_operand);
        let carry_flag_byte = emulator.carry_flag_byte_value();

        let low_bit = r8_value & 0x01;
        let rotated_reg = (r8_value >> 1) | (carry_flag_byte << 7);
        emulator.write_r8_operand_value(r8_operand, rotated_reg);

        emulator.regs_mut().set_carry_flag(low_bit != 0);
        emulator.set_zero_flag_for_value(rotated_reg);
        emulator.regs_mut().set_bcd_flags_zero();

        let num_ticks = 8 + double_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let r8_operand = low_r8_operand(instrs[0]);

        formatter.opcode("rr");
        formatter.r8_operand(r8_operand);
    },
);

define_instruction!(
    sla,
    fn execute(emulator, opcode) {
        // Arithmetically shift register left setting carry flag with shifted bit.
        let r8_operand = low_r8_operand(opcode);
        let r8_value = emulator.read_r8_operand_value(r8_operand);

        let high_bit = r8_value & 0x80;
        let shifted_reg = r8_value << 1;
        emulator.write_r8_operand_value(r8_operand, shifted_reg);

        emulator.regs_mut().set_carry_flag(high_bit != 0);
        emulator.set_zero_flag_for_value(shifted_reg);
        emulator.regs_mut().set_bcd_flags_zero();

        let num_ticks = 8 + double_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let r8_operand = low_r8_operand(instrs[0]);

        formatter.opcode("sla");
        formatter.r8_operand(r8_operand);
    },
);

define_instruction!(
    sra,
    fn execute(emulator, opcode) {
        // Arithmetically shift register right setting carry flag with shifted bit.
        let r8_operand = low_r8_operand(opcode);
        let r8_value = emulator.read_r8_operand_value(r8_operand);

        let low_bit = r8_value & 0x01;
        let shifted_reg = ((r8_value as i8) >> 1) as u8;
        emulator.write_r8_operand_value(r8_operand, shifted_reg);

        emulator.regs_mut().set_carry_flag(low_bit != 0);
        emulator.set_zero_flag_for_value(shifted_reg);
        emulator.regs_mut().set_bcd_flags_zero();

        let num_ticks = 8 + double_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let r8_operand = low_r8_operand(instrs[0]);

        formatter.opcode("sra");
        formatter.r8_operand(r8_operand);
    },
);

define_instruction!(
    srl,
    fn execute(emulator, opcode) {
        // Logically shift register right setting carry flag with shifted bit.
        let r8_operand = low_r8_operand(opcode);
        let r8_value = emulator.read_r8_operand_value(r8_operand);

        let low_bit = r8_value & 0x01;
        let shifted_reg = r8_value >> 1;
        emulator.write_r8_operand_value(r8_operand, shifted_reg);

        emulator.regs_mut().set_carry_flag(low_bit != 0);
        emulator.set_zero_flag_for_value(shifted_reg);
        emulator.regs_mut().set_bcd_flags_zero();

        let num_ticks = 8 + double_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let r8_operand = low_r8_operand(instrs[0]);

        formatter.opcode("srl");
        formatter.r8_operand(r8_operand);
    },
);

define_instruction!(
    swap,
    fn execute (emulator, opcode) {
        let r8_operand = low_r8_operand(opcode);
        let r8_value = emulator.read_r8_operand_value(r8_operand);

        let high_nibble = r8_value >> 4;
        let low_nibble = r8_value & 0x0F;

        let result = (low_nibble << 4) | high_nibble;
        emulator.write_r8_operand_value(r8_operand, result);

        emulator.set_zero_flag_for_value(result);
        emulator.regs_mut().set_carry_flag(false);
        emulator.regs_mut().set_bcd_flags_zero();

        let num_ticks = 8 + double_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let r8_operand = low_r8_operand(instrs[0]);

        formatter.opcode("swap");
        formatter.r8_operand(r8_operand);
    },
);

define_instruction!(
    bit,
    fn execute (emulator, opcode) {
        let r8_operand = low_r8_operand(opcode);
        let bit_index = bit_index_operand(opcode);

        let r8_value = emulator.read_r8_operand_value(r8_operand);
        let is_bit_zero = r8_value & (1 << bit_index) == 0;

        emulator.regs_mut().set_zero_flag(is_bit_zero);
        emulator.regs_mut().set_subtraction_flag(false);
        emulator.regs_mut().set_half_carry_flag(true);

        let num_ticks = 8 + single_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let r8_operand = low_r8_operand(instrs[0]);
        let bit_index_operand = bit_index_operand(instrs[0]);

        formatter.opcode("bit");
        formatter.bit_index_operand(bit_index_operand);
        formatter.comma();
        formatter.r8_operand(r8_operand);
    },
);

define_instruction!(
    res,
    fn execute (emulator, opcode) {
        let r8_operand = low_r8_operand(opcode);
        let bit_index = bit_index_operand(opcode);

        // Set the bit at the given index to 0
        let r8_value = emulator.read_r8_operand_value(r8_operand);
        let result = r8_value & !(1 << bit_index);
        emulator.write_r8_operand_value(r8_operand, result);

        let num_ticks = 8 + double_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let r8_operand = low_r8_operand(instrs[0]);
        let bit_index_operand = bit_index_operand(instrs[0]);

        formatter.opcode("res");
        formatter.bit_index_operand(bit_index_operand);
        formatter.comma();
        formatter.r8_operand(r8_operand);
    },
);

define_instruction!(
    set,
    fn execute (emulator, opcode) {
        let r8_operand = low_r8_operand(opcode);
        let bit_index = bit_index_operand(opcode);

        // Set the bit at the given index to 1
        let r8_value = emulator.read_r8_operand_value(r8_operand);
        let result = r8_value | (1 << bit_index);
        emulator.write_r8_operand_value(r8_operand, result);

        let num_ticks = 8 + double_r8_operand_cycles(r8_operand);
        emulator.schedule_next_instruction(num_ticks);
    },
    fn format(instrs, formatter) {
        let r8_operand = low_r8_operand(instrs[0]);
        let bit_index_operand = bit_index_operand(instrs[0]);

        formatter.opcode("set");
        formatter.bit_index_operand(bit_index_operand);
        formatter.comma();
        formatter.r8_operand(r8_operand);
    },
);

// An opcode that does not match any valid instruction.
define_instruction!(
    invalid,
    fn execute (_, opcode) {
        panic!("Invalid opcode {:02X}", opcode);
    },
    fn format(_, formatter) {
       formatter.simple_opcode("<invalid>");
    },
);

macro_rules! define_instructions {
    ($($instr:ident,)*) => {
        /// Jump table from opcode to the instruction handler.
        const DISPATCH_TABLE: [InstructionHandler; 256] = [
            $($instr::execute,)*
        ];

        /// Jump table from opcode to its formatter.
        const INSTRUCTION_FORMATTERS: [FormatterHandler; 256] = [
            $($instr::format,)*
        ];
    }
}

macro_rules! define_cb_instructions {
    ($($instr:ident,)*) => {
        const CB_DISPATCH_TABLE: [InstructionHandler; 256] = [
            $($instr::execute,)*
        ];

        const CB_FORMATTER_TABLE: [FormatterHandler; 256] = [
            $($instr::format,)*
        ];
    }
}

#[rustfmt::skip]
define_instructions!(
    /////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    //         //     0x0      //     0x1      //     0x2      //     0x3      //     0x4      //     0x5      //     0x6      //     0x7      //     0x8      //     0x9      //     0xA      //     0xB      //     0xC      //     0xD      //     0xE      //     0xF      //
    /////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    /* 0x00 */ nop,            ld_r16_imm16,   ld_r16mem_a,    inc_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     rlca,           ld_imm16mem_sp, add_hl_r16,     ld_a_r16mem,    dec_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     rrca,
    /* 0x10 */ stop,           ld_r16_imm16,   ld_r16mem_a,    inc_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     rla,            jr_imm8,        add_hl_r16,     ld_a_r16mem,    dec_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     rra,
    /* 0x20 */ jr_cc_imm8,     ld_r16_imm16,   ld_hli_a,       inc_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     daa,            jr_cc_imm8,     add_hl_r16,     ld_a_hli,       dec_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     cpl,
    /* 0x30 */ jr_cc_imm8,     ld_r16_imm16,   ld_hld_a,       inc_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     scf,            jr_cc_imm8,     add_hl_r16,     ld_a_hld,       dec_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     ccf,
    /* 0x40 */ ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,
    /* 0x50 */ ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,
    /* 0x60 */ ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,
    /* 0x70 */ ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       halt,           ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,       ld_r8_r8,
    /* 0x80 */ add_a_r8,       add_a_r8,       add_a_r8,       add_a_r8,       add_a_r8,       add_a_r8,       add_a_r8,       add_a_r8,       adc_a_r8,       adc_a_r8,       adc_a_r8,       adc_a_r8,       adc_a_r8,       adc_a_r8,       adc_a_r8,       adc_a_r8,
    /* 0x90 */ sub_a_r8,       sub_a_r8,       sub_a_r8,       sub_a_r8,       sub_a_r8,       sub_a_r8,       sub_a_r8,       sub_a_r8,       sbc_a_r8,       sbc_a_r8,       sbc_a_r8,       sbc_a_r8,       sbc_a_r8,       sbc_a_r8,       sbc_a_r8,       sbc_a_r8,
    /* 0xA0 */ and_a_r8,       and_a_r8,       and_a_r8,       and_a_r8,       and_a_r8,       and_a_r8,       and_a_r8,       and_a_r8,       xor_a_r8,       xor_a_r8,       xor_a_r8,       xor_a_r8,       xor_a_r8,       xor_a_r8,       xor_a_r8,       xor_a_r8,
    /* 0xB0 */ or_a_r8,        or_a_r8,        or_a_r8,        or_a_r8,        or_a_r8,        or_a_r8,        or_a_r8,        or_a_r8,        cp_a_r8,        cp_a_r8,        cp_a_r8,        cp_a_r8,        cp_a_r8,        cp_a_r8,        cp_a_r8,        cp_a_r8,
    /* 0xC0 */ ret_cc,         pop_r16,        jp_cond_imm16,  jp_imm16,       call_cc_imm16,  push_r16,       add_a_imm8,     rst_tgt,        ret_cc,         ret,            jp_cond_imm16,  cb_prefix,      call_cc_imm16,  call_imm16,     adc_a_imm8,     rst_tgt,
    /* 0xD0 */ ret_cc,         pop_r16,        jp_cond_imm16,  invalid,        call_cc_imm16,  push_r16,       sub_a_imm8,     rst_tgt,        ret_cc,         reti,           jp_cond_imm16,  invalid,        call_cc_imm16,  invalid,        sbc_a_imm8,     rst_tgt,
    /* 0xE0 */ ldh_imm8mem_a,  pop_r16,        ldh_cmem_a,     invalid,        invalid,        push_r16,       and_a_imm8,     rst_tgt,        add_sp_imm8,    jp_hl,          ld_imm16mem_a,  invalid,        invalid,        invalid,        xor_a_imm8,     rst_tgt,
    /* 0xF0 */ ldh_a_imm8mem,  pop_af,         ldh_a_cmem,     di,             invalid,        push_af,        or_a_imm8,      rst_tgt,        ld_hl_sp_imm8,  ld_sp_hl,       ld_a_imm16mem,  ei,             invalid,        invalid,        cp_a_imm8,      rst_tgt,
);

#[rustfmt::skip]
define_cb_instructions!(
    ////////// 0x00  0x01  0x02  0x03  0x04  0x05  0x06  0x07  0x08  0x09  0x0A  0x0B  0x0C  0x0D  0x0E  0x0F
    /* 0x00 */ rlc,  rlc,  rlc,  rlc,  rlc,  rlc,  rlc,  rlc,  rrc,  rrc,  rrc,  rrc,  rrc,  rrc,  rrc,  rrc,
    /* 0x10 */ rl,   rl,   rl,   rl,   rl,   rl,   rl,   rl,   rr,   rr,   rr,   rr,   rr,   rr,   rr,   rr,
    /* 0x20 */ sla,  sla,  sla,  sla,  sla,  sla,  sla,  sla,  sra,  sra,  sra,  sra,  sra,  sra,  sra,  sra,
    /* 0x30 */ swap, swap, swap, swap, swap, swap, swap, swap, srl,  srl,  srl,  srl,  srl,  srl,  srl,  srl,
    /* 0x40 */ bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,
    /* 0x50 */ bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,
    /* 0x60 */ bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,
    /* 0x70 */ bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,  bit,
    /* 0x80 */ res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,
    /* 0x90 */ res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,
    /* 0xA0 */ res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,
    /* 0xB0 */ res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,  res,
    /* 0xC0 */ set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,
    /* 0xD0 */ set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,
    /* 0xE0 */ set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,
    /* 0xF0 */ set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,  set,
);

#[cfg(test)]
mod test {
    use super::InstructionFormatter;

    #[test]
    pub fn format_all_opcodes() {
        // Create an exhaustive list of all instructions (and cb-prefixed instructions)
        let mut instrs = vec![];
        let mut cb_instrs = vec![];

        for i in 0..=255 {
            instrs.push(i);

            cb_instrs.push(0xCB);
            cb_instrs.push(i);
        }

        // Format each instruction and compare to snapshots
        let mut formatted_instrs = vec![];
        let mut formatted_cb_instrs = vec![];

        for i in 0..256 {
            formatted_instrs.push(InstructionFormatter::format(&instrs[i..]));
            formatted_cb_instrs.push(InstructionFormatter::format(&cb_instrs[(i * 2)..]));
        }

        for i in 0..16 {
            let actual_instrs = formatted_instrs[(i * 16)..((i + 1) * 16)].join(" | ");
            assert_eq!(EXPECTED_INSTRS[i], actual_instrs);
        }

        for i in 0..16 {
            let actual_instrs = formatted_cb_instrs[(i * 16)..((i + 1) * 16)].join(" | ");
            assert_eq!(EXPECTED_CB_INSTRS[i], actual_instrs);
        }
    }

    const EXPECTED_INSTRS: [&'static str; 16] = [
        "nop | ld bc, #770 | ld [bc], a | inc bc | inc b | dec b | ld b, #7 | rlca | ld [#2569], sp | add hl, bc | ld a, [bc] | dec bc | inc c | dec c | ld c, #15 | rrca",
        "stop | ld de, #4882 | ld [de], a | inc de | inc d | dec d | ld d, #23 | rla | jr #25 | add hl, de | ld a, [de] | dec de | inc e | dec e | ld e, #31 | rra",
        "jr nz, #33 | ld hl, #8994 | ld [hl+], a | inc hl | inc h | dec h | ld h, #39 | daa | jr z, #41 | add hl, hl | ld a, [hl+] | dec hl | inc l | dec l | ld l, #47 | cpl",
        "jr nc, #49 | ld sp, #13106 | ld [hl-], a | inc sp | inc [hl] | dec [hl] | ld [hl], #55 | scf | jr c, #57 | add hl, sp | ld a, [hl-] | dec sp | inc a | dec a | ld a, #63 | ccf",
        "ld b, b | ld b, c | ld b, d | ld b, e | ld b, h | ld b, l | ld b, [hl] | ld b, a | ld c, b | ld c, c | ld c, d | ld c, e | ld c, h | ld c, l | ld c, [hl] | ld c, a",
        "ld d, b | ld d, c | ld d, d | ld d, e | ld d, h | ld d, l | ld d, [hl] | ld d, a | ld e, b | ld e, c | ld e, d | ld e, e | ld e, h | ld e, l | ld e, [hl] | ld e, a",
        "ld h, b | ld h, c | ld h, d | ld h, e | ld h, h | ld h, l | ld h, [hl] | ld h, a | ld l, b | ld l, c | ld l, d | ld l, e | ld l, h | ld l, l | ld l, [hl] | ld l, a",
        "ld [hl], b | ld [hl], c | ld [hl], d | ld [hl], e | ld [hl], h | ld [hl], l | halt | ld [hl], a | ld a, b | ld a, c | ld a, d | ld a, e | ld a, h | ld a, l | ld a, [hl] | ld a, a",
        "add a, b | add a, c | add a, d | add a, e | add a, h | add a, l | add a, [hl] | add a, a | adc a, b | adc a, c | adc a, d | adc a, e | adc a, h | adc a, l | adc a, [hl] | adc a, a",
        "sub a, b | sub a, c | sub a, d | sub a, e | sub a, h | sub a, l | sub a, [hl] | sub a, a | sbc a, b | sbc a, c | sbc a, d | sbc a, e | sbc a, h | sbc a, l | sbc a, [hl] | sbc a, a",
        "and a, b | and a, c | and a, d | and a, e | and a, h | and a, l | and a, [hl] | and a, a | xor a, b | xor a, c | xor a, d | xor a, e | xor a, h | xor a, l | xor a, [hl] | xor a, a",
        "or a, b | or a, c | or a, d | or a, e | or a, h | or a, l | or a, [hl] | or a, a | cp a, b | cp a, c | cp a, d | cp a, e | cp a, h | cp a, l | cp a, [hl] | cp a, a",
        "ret nz | pop bc | jp nz, #50371 | jp #50628 | call nz, #50885 | push bc | add a, #199 | rst 0x00 | ret z | ret | jp z, #52427 | set 1, h | call z, #52941 | call #53198 | adc a, #207 | rst 0x08",
        "ret nc | pop de | jp nc, #54483 | <invalid> | call nc, #54997 | push de | sub a, #215 | rst 0x10 | ret c | reti | jp c, #56539 | <invalid> | call c, #57053 | <invalid> | sbc a, #223 | rst 0x18",
        "ldh [#225], a | pop hl | ldh [c], a | <invalid> | <invalid> | push hl | and a, #231 | rst 0x20 | add sp, #-23 | jp hl | ld [#60651], a | <invalid> | <invalid> | <invalid> | xor a, #239 | rst 0x28",
        "ldh a, [#241] | pop af | ldh a, [c] | di  | <invalid> | push af | or a, #247 | rst 0x30 | ld hl, sp + #-7 | ld sp, hl | ld a, [#64763] | ei  | <invalid> | <invalid> | cp a, #255 | rst 0x38",
    ];

    const EXPECTED_CB_INSTRS: [&'static str; 16] = [
        "rlc b | rlc c | rlc d | rlc e | rlc h | rlc l | rlc [hl] | rlc a | rrc b | rrc c | rrc d | rrc e | rrc h | rrc l | rrc [hl] | rrc a",
        "rl b | rl c | rl d | rl e | rl h | rl l | rl [hl] | rl a | rr b | rr c | rr d | rr e | rr h | rr l | rr [hl] | rr a",
        "sla b | sla c | sla d | sla e | sla h | sla l | sla [hl] | sla a | sra b | sra c | sra d | sra e | sra h | sra l | sra [hl] | sra a",
        "swap b | swap c | swap d | swap e | swap h | swap l | swap [hl] | swap a | srl b | srl c | srl d | srl e | srl h | srl l | srl [hl] | srl a",
        "bit 0, b | bit 0, c | bit 0, d | bit 0, e | bit 0, h | bit 0, l | bit 0, [hl] | bit 0, a | bit 1, b | bit 1, c | bit 1, d | bit 1, e | bit 1, h | bit 1, l | bit 1, [hl] | bit 1, a",
        "bit 2, b | bit 2, c | bit 2, d | bit 2, e | bit 2, h | bit 2, l | bit 2, [hl] | bit 2, a | bit 3, b | bit 3, c | bit 3, d | bit 3, e | bit 3, h | bit 3, l | bit 3, [hl] | bit 3, a",
        "bit 4, b | bit 4, c | bit 4, d | bit 4, e | bit 4, h | bit 4, l | bit 4, [hl] | bit 4, a | bit 5, b | bit 5, c | bit 5, d | bit 5, e | bit 5, h | bit 5, l | bit 5, [hl] | bit 5, a",
        "bit 6, b | bit 6, c | bit 6, d | bit 6, e | bit 6, h | bit 6, l | bit 6, [hl] | bit 6, a | bit 7, b | bit 7, c | bit 7, d | bit 7, e | bit 7, h | bit 7, l | bit 7, [hl] | bit 7, a",
        "res 0, b | res 0, c | res 0, d | res 0, e | res 0, h | res 0, l | res 0, [hl] | res 0, a | res 1, b | res 1, c | res 1, d | res 1, e | res 1, h | res 1, l | res 1, [hl] | res 1, a",
        "res 2, b | res 2, c | res 2, d | res 2, e | res 2, h | res 2, l | res 2, [hl] | res 2, a | res 3, b | res 3, c | res 3, d | res 3, e | res 3, h | res 3, l | res 3, [hl] | res 3, a",
        "res 4, b | res 4, c | res 4, d | res 4, e | res 4, h | res 4, l | res 4, [hl] | res 4, a | res 5, b | res 5, c | res 5, d | res 5, e | res 5, h | res 5, l | res 5, [hl] | res 5, a",
        "res 6, b | res 6, c | res 6, d | res 6, e | res 6, h | res 6, l | res 6, [hl] | res 6, a | res 7, b | res 7, c | res 7, d | res 7, e | res 7, h | res 7, l | res 7, [hl] | res 7, a",
        "set 0, b | set 0, c | set 0, d | set 0, e | set 0, h | set 0, l | set 0, [hl] | set 0, a | set 1, b | set 1, c | set 1, d | set 1, e | set 1, h | set 1, l | set 1, [hl] | set 1, a",
        "set 2, b | set 2, c | set 2, d | set 2, e | set 2, h | set 2, l | set 2, [hl] | set 2, a | set 3, b | set 3, c | set 3, d | set 3, e | set 3, h | set 3, l | set 3, [hl] | set 3, a",
        "set 4, b | set 4, c | set 4, d | set 4, e | set 4, h | set 4, l | set 4, [hl] | set 4, a | set 5, b | set 5, c | set 5, d | set 5, e | set 5, h | set 5, l | set 5, [hl] | set 5, a",
        "set 6, b | set 6, c | set 6, d | set 6, e | set 6, h | set 6, l | set 6, [hl] | set 6, a | set 7, b | set 7, c | set 7, d | set 7, e | set 7, h | set 7, l | set 7, [hl] | set 7, a",
    ];
}
