use crate::emulator::Emulator;

pub mod registers;

impl Emulator {
    /// Execute an instruction, returning the number of clock cycles taken by the instruction.
    pub fn execute_instruction(&mut self) {
        let opcode = self.read_opcode();
        DISPATCH_TABLE[opcode as usize](self, opcode);
    }

    /// Read the opcode at PC and advance PC to the following byte.
    fn read_opcode(&mut self) -> u8 {
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
    fn get_r8_operand_value(&self, r8_operand: u8) -> u8 {
        match r8_operand {
            0 => self.regs().b(),
            1 => self.regs().c(),
            2 => self.regs().d(),
            3 => self.regs().e(),
            4 => self.regs().h(),
            5 => self.regs().l(),
            R8_OPERAND_HL_MEM => self.read_address(self.regs().hl()),
            7 => self.regs().a(),
            _ => unreachable!("Invalid r8 operand"),
        }
    }

    /// Write a value to the specified 8-bit register operand.
    ///
    /// `r8_operand` must be in the range 0-7.
    fn write_r8_operand_value(&mut self, r8_operand: u8, value: u8) {
        match r8_operand {
            0 => self.regs_mut().set_b(value),
            1 => self.regs_mut().set_c(value),
            2 => self.regs_mut().set_d(value),
            3 => self.regs_mut().set_e(value),
            4 => self.regs_mut().set_h(value),
            5 => self.regs_mut().set_l(value),
            R8_OPERAND_HL_MEM => self.write_address(self.regs().hl(), value),
            7 => self.regs_mut().set_a(value),
            _ => unreachable!("Invalid r8 operand"),
        }
    }

    /// Get the value of the specified 16-bit register operand.
    ///
    /// `r16_operand` must be in the range 0-3.
    fn get_r16_operand_value(&self, r16_operand: u8) -> u16 {
        match r16_operand {
            0 => self.regs().bc(),
            1 => self.regs().de(),
            2 => self.regs().hl(),
            3 => self.regs().sp(),
            _ => unreachable!("Invalid r16 operand"),
        }
    }

    /// Write a value to the specified 16-bit register operand.
    ///
    /// `r16_operand` must be in the range 0-3.
    fn write_r16_operand_value(&mut self, r16_operand: u8, value: u16) {
        match r16_operand {
            0 => self.regs_mut().set_bc(value),
            1 => self.regs_mut().set_de(value),
            2 => self.regs_mut().set_hl(value),
            3 => self.regs_mut().set_sp(value),
            _ => unreachable!("Invalid r16 operand"),
        }
    }
}

type Opcode = u8;
type InstructionHandler = fn(&mut Emulator, Opcode);

macro_rules! define_instruction {
    ($name:ident, fn ($emulator:pat, $opcode:pat) $body:block) => {
        fn $name($emulator: &mut Emulator, $opcode: Opcode) {
            $body
        }
    };
}

macro_rules! unimplemented_instruction {
    ($name:ident) => {
        fn $name(_: &mut Emulator, _: Opcode) {
            unimplemented!(stringify!($name));
        }
    };
}

define_instruction!(nop, fn (emulator, _) {
    // Do nothing
    emulator.schedule_next_instruction(4);
});

unimplemented_instruction!(ld_r16_imm16);

unimplemented_instruction!(ld_r16mem_a);

unimplemented_instruction!(ld_a_r16mem);

unimplemented_instruction!(ld_imm16mem_sp);

define_instruction!(inc_r16, fn (emulator, operand) {
    let r16_operand = r16_operand(operand);
    let r16_value = emulator.get_r16_operand_value(r16_operand);

    let result = r16_value.wrapping_add(1);
    emulator.write_r16_operand_value(r16_operand, result);

    emulator.schedule_next_instruction(8);
});

define_instruction!(dec_r16, fn (emulator, operand) {
    let r16_operand = r16_operand(operand);
    let r16_value = emulator.get_r16_operand_value(r16_operand);

    let result = r16_value.wrapping_sub(1);
    emulator.write_r16_operand_value(r16_operand, result);

    emulator.schedule_next_instruction(8);
});

define_instruction!(inc_r8, fn (emulator, operand) {
    let r8_operand = high_r8_operand(operand);
    let r8_value = emulator.get_r8_operand_value(r8_operand);

    let result = r8_value.wrapping_add(1);
    emulator.write_r8_operand_value(r8_operand, result);

    // Carry flag is not set
    emulator.set_zero_flag_for_value(result);

    let num_ticks = 4 + (r8_operand_cycles(r8_operand) * 2);
    emulator.schedule_next_instruction(num_ticks);
});

define_instruction!(dec_r8, fn (emulator, operand) {
    let r8_operand = high_r8_operand(operand);
    let r8_value = emulator.get_r8_operand_value(r8_operand);

    let result = r8_value.wrapping_sub(1);
    emulator.write_r8_operand_value(r8_operand, result);

    // Carry flag is not set
    emulator.set_zero_flag_for_value(result);

    let num_ticks = 4 + (r8_operand_cycles(r8_operand) * 2);
    emulator.schedule_next_instruction(num_ticks);
});

define_instruction!(add_hl_r16, fn (emulator, opcode) {
    let r16_operand = r16_operand(opcode);
    let r16_value = emulator.get_r16_operand_value(r16_operand);
    let hl = emulator.regs().hl();

    let (result, carried) = hl.overflowing_add(r16_value);
    emulator.regs_mut().set_hl(result);

    // Zero flag is not set
    emulator.regs_mut().set_carry_flag(carried);

    emulator.schedule_next_instruction(8);
});

unimplemented_instruction!(ld_r8_imm8);

unimplemented_instruction!(rlca);
unimplemented_instruction!(rrca);
unimplemented_instruction!(rla);
unimplemented_instruction!(rra);
unimplemented_instruction!(daa);
unimplemented_instruction!(cpl);
unimplemented_instruction!(scf);
unimplemented_instruction!(ccf);

unimplemented_instruction!(jr_imm8);
unimplemented_instruction!(jr_cc_imm8);

unimplemented_instruction!(stop);

unimplemented_instruction!(ld_r8_r8);

unimplemented_instruction!(halt);

/// An r8 operand encoded in bits 0-2 of the opcode.
fn low_r8_operand(opcode: Opcode) -> u8 {
    opcode & 0x07
}

/// An r8 operand encoded in bits 3-5 of the opcode.
fn high_r8_operand(opcode: Opcode) -> u8 {
    (opcode >> 3) & 0x07
}

/// All possible r16 operands are encoded in bits 4-5 of the opcode.
fn r16_operand(opcode: Opcode) -> u8 {
    (opcode >> 4) & 0x03
}

const R8_OPERAND_HL_MEM: u8 = 6;

/// The number of cycles added for this r8 operand. Only reading from address at HL adds cycles.
fn r8_operand_cycles(r8_operand: u8) -> usize {
    if r8_operand == R8_OPERAND_HL_MEM {
        4
    } else {
        0
    }
}

/// Any arithmetic operation between the accumulator and an r8 operand.
fn arithmetic_a_r8_instruction(
    emulator: &mut Emulator,
    opcode: Opcode,
    operation: fn(&mut Emulator, u8, u8) -> u8,
) {
    let r8_operand = low_r8_operand(opcode);
    let r8_value = emulator.get_r8_operand_value(r8_operand);
    let acc = emulator.regs().a();

    let result = operation(emulator, acc, r8_value);

    emulator.regs_mut().set_a(result);
    emulator.set_zero_flag_for_value(result);

    let num_ticks = 4 + r8_operand_cycles(r8_operand);
    emulator.schedule_next_instruction(num_ticks);
}

define_instruction!(add_a_r8, fn (emulator, opcode) {
    arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
        let (result, carried) = acc.overflowing_add(r8_value);
        emulator.regs_mut().set_carry_flag(carried);
        result
    });
});

define_instruction!(sub_a_r8, fn (emulator, opcode) {
    arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
        let (result, carried) = acc.overflowing_sub(r8_value);
        emulator.regs_mut().set_carry_flag(carried);
        result
    });
});

define_instruction!(adc_a_r8, fn (emulator, opcode) {
    arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
        let (tmp, carry1) = acc.overflowing_add(r8_value);
        let (result, carry2) = tmp.overflowing_add(emulator.carry_flag_byte_value());
        emulator.regs_mut().set_carry_flag(carry1 || carry2);
        result
    });
});

define_instruction!(sbc_a_r8, fn (emulator, opcode) {
    arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
        let (tmp, carry1) = acc.overflowing_sub(r8_value);
        let (result, carry2) = tmp.overflowing_sub(emulator.carry_flag_byte_value());
        emulator.regs_mut().set_carry_flag(carry1 || carry2);
        result
    });
});

define_instruction!(and_a_r8, fn (emulator, opcode) {
    arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
        emulator.regs_mut().set_carry_flag(false);
        acc & r8_value
    });
});

define_instruction!(xor_a_r8, fn (emulator, opcode) {
    arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
        emulator.regs_mut().set_carry_flag(false);
        acc ^ r8_value
    });
});

define_instruction!(or_a_r8, fn (emulator, opcode) {
    arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
        emulator.regs_mut().set_carry_flag(false);
        acc | r8_value
    });
});

// Identical to sub_a_r8, but does not write the result to the accumulator.
define_instruction!(cp_a_r8, fn (emulator, opcode) {
    let r8_operand = low_r8_operand(opcode);
    let r8_value = emulator.get_r8_operand_value(r8_operand);
    let acc = emulator.regs().a();

    let (result, carried) = acc.overflowing_sub(r8_value);
    emulator.regs_mut().set_carry_flag(carried);
    emulator.set_zero_flag_for_value(result);

    let num_ticks = 4 + r8_operand_cycles(r8_operand);
    emulator.schedule_next_instruction(num_ticks);
});

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

define_instruction!(add_a_imm8, fn (emulator, _) {
    arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
        let (result, carried) = acc.overflowing_add(imm8_value);
        emulator.regs_mut().set_carry_flag(carried);
        result
    });
});

define_instruction!(sub_a_imm8, fn (emulator, _) {
    arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
        let (result, carried) = acc.overflowing_sub(imm8_value);
        emulator.regs_mut().set_carry_flag(carried);
        result
    });
});

define_instruction!(adc_a_imm8, fn (emulator, _) {
    arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
        let (tmp, carry1) = acc.overflowing_add(imm8_value);
        let (result, carry2) = tmp.overflowing_add(emulator.carry_flag_byte_value());
        emulator.regs_mut().set_carry_flag(carry1 || carry2);
        result
    });
});

define_instruction!(sbc_a_imm8, fn (emulator, _) {
    arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
        let (tmp, carry1) = acc.overflowing_sub(imm8_value);
        let (result, carry2) = tmp.overflowing_sub(emulator.carry_flag_byte_value());
        emulator.regs_mut().set_carry_flag(carry1 || carry2);
        result
    });
});

define_instruction!(and_a_imm8, fn (emulator, _) {
    arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
        emulator.regs_mut().set_carry_flag(false);
        acc & imm8_value
    });
});

define_instruction!(xor_a_imm8, fn (emulator, _) {
    arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
        emulator.regs_mut().set_carry_flag(false);
        acc ^ imm8_value
    });
});

define_instruction!(or_a_imm8, fn (emulator, _) {
    arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
        emulator.regs_mut().set_carry_flag(false);
        acc | imm8_value
    });
});

// Identical to sub_a_imm8, but does not write the result to the accumulator.
define_instruction!(cp_a_imm8, fn (emulator, _) {
    let imm8_value = emulator.read_imm8_operand();
    let acc = emulator.regs().a();

    let (result, carried) = acc.overflowing_sub(imm8_value);
    emulator.regs_mut().set_carry_flag(carried);
    emulator.set_zero_flag_for_value(result);

    emulator.schedule_next_instruction(8);
});

unimplemented_instruction!(ret_cc);
unimplemented_instruction!(ret);
unimplemented_instruction!(reti);
unimplemented_instruction!(jp_cond_imm16);

define_instruction!(jp_imm16, fn (emulator, _) {
    let imm16 = emulator.read_imm16_operand();
    emulator.regs_mut().set_pc(imm16);

    emulator.schedule_next_instruction(16);
});

unimplemented_instruction!(jp_hl);
unimplemented_instruction!(call_cc_imm16);
unimplemented_instruction!(call_imm16);
unimplemented_instruction!(rst_tgt);

unimplemented_instruction!(pop_r16);
unimplemented_instruction!(pop_af);
unimplemented_instruction!(push_r16);
unimplemented_instruction!(push_af);

unimplemented_instruction!(cb_prefix);

unimplemented_instruction!(ldh_cmem_a);
unimplemented_instruction!(ldh_imm8mem_a);
unimplemented_instruction!(ld_imm16mem_a);
unimplemented_instruction!(ldh_a_cmem);
unimplemented_instruction!(ldh_a_imm8mem);
unimplemented_instruction!(ld_a_imm16mem);

define_instruction!(add_sp_imm8, fn (emulator, _) {
    let signed_operand = emulator.read_imm8_operand() as i8 as i16;
    let sp = emulator.regs().sp();

    let (result, carried) = sp.overflowing_add_signed(signed_operand);
    emulator.regs_mut().set_sp(result);

    emulator.regs_mut().set_zero_flag(false);
    emulator.regs_mut().set_carry_flag(carried);

    emulator.schedule_next_instruction(16);
});

unimplemented_instruction!(ld_hl_sp_imm8);
unimplemented_instruction!(ld_sp_hl);

define_instruction!(di, fn (emulator, _) {
    emulator.regs_mut().set_interrupts_enabled(false);

    emulator.schedule_next_instruction(4);
});

unimplemented_instruction!(ei);

// An opcode that does not match any valid instruction.
define_instruction!(invalid, fn (_, opcode) {
    panic!("Invalid opcode {:02X}", opcode);
});

/// Jump table from opcode to the instruction handler.
#[rustfmt::skip]
const DISPATCH_TABLE: [InstructionHandler; 256] = [
    /////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    //         //     0x0      //     0x1      //     0x2      //     0x3      //     0x4      //     0x5      //     0x6      //     0x7      //     0x8      //     0x9      //     0xA      //     0xB      //     0xC      //     0xD      //     0xE      //     0xF      //
    /////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
    /* 0x00 */ nop,            ld_r16_imm16,   ld_r16mem_a,    inc_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     rlca,           ld_imm16mem_sp, add_hl_r16,     ld_a_r16mem,    dec_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     rrca,
    /* 0x10 */ stop,           ld_r16_imm16,   ld_r16mem_a,    inc_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     rla,            jr_imm8,        add_hl_r16,     ld_a_r16mem,    dec_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     rra,
    /* 0x20 */ jr_cc_imm8,     ld_r16_imm16,   ld_r16mem_a,    inc_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     daa,            jr_cc_imm8,     add_hl_r16,     ld_a_r16mem,    dec_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     cpl,
    /* 0x30 */ jr_cc_imm8,     ld_r16_imm16,   ld_r16mem_a,    inc_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     scf,            jr_cc_imm8,     add_hl_r16,     ld_a_r16mem,    dec_r16,        inc_r8,         dec_r8,         ld_r8_imm8,     ccf,
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
];
