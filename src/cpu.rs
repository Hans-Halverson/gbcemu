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
            7 => self.regs().a(),
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
            7 => self.regs_mut().set_a(value),
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
            2 => self.regs().hl(),
            3 => self.regs().sp(),
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
            2 => self.regs_mut().set_hl(value),
            3 => self.regs_mut().set_sp(value),
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

type Opcode = u8;
type R8Operand = u8;
type R16Operand = u8;
type CcOperand = u8;
type InstructionHandler = fn(&mut Emulator, Opcode);

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

const R8_OPERAND_HL_MEM: R8Operand = 6;

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
fn half_carry_for_sp_i8_add(a: u16, b: i8) -> bool {
    if b >= 0 {
        half_carry_for_add2(a as u8, b as u8)
    } else {
        half_carry_for_sub2(a as u8, b.wrapping_neg() as u8)
    }
}

macro_rules! define_instruction {
    ($name:ident, fn ($emulator:pat, $opcode:pat) $body:block) => {
        fn $name($emulator: &mut Emulator, $opcode: Opcode) {
            $body
        }
    };
}

define_instruction!(nop, fn (emulator, _) {
    // Do nothing
    emulator.schedule_next_instruction(4);
});

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

define_instruction!(ld_r8_r8, fn (emulator, opcode) {
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
});

define_instruction!(ld_r8_imm8, fn (emulator, opcode) {
    let r8_operand = high_r8_operand(opcode);
    let imm8_value = emulator.read_imm8_operand();

    emulator.write_r8_operand_value(r8_operand, imm8_value);

    let num_ticks = 8 + single_r8_operand_cycles(r8_operand);
    emulator.schedule_next_instruction(num_ticks);
});

define_instruction!(ld_r16_imm16, fn (emulator, opcode) {
    let r16_operand = r16_operand(opcode);
    let imm16_value = emulator.read_imm16_operand();

    emulator.write_r16_operand_value(r16_operand, imm16_value);

    emulator.schedule_next_instruction(12);
});

define_instruction!(ld_r16mem_a, fn (emulator, opcode) {
    let r16_operand = r16_operand(opcode);
    let r16_value = emulator.read_r16_operand_value(r16_operand);

    let accumulator = emulator.regs().a();
    emulator.write_address(r16_value, accumulator);

    emulator.schedule_next_instruction(8);
});

define_instruction!(ld_a_r16mem, fn (emulator, opcode) {
    let r16_operand = r16_operand(opcode);
    let r16_value = emulator.read_r16_operand_value(r16_operand);

    let r16_mem = emulator.read_address(r16_value);
    emulator.regs_mut().set_a(r16_mem);

    emulator.schedule_next_instruction(8);
});

define_instruction!(ld_imm16mem_a, fn(emulator, _) {
    let imm16 = emulator.read_imm16_operand();
    let accumulator = emulator.regs().a();

    emulator.write_address(imm16, accumulator);

    emulator.schedule_next_instruction(16);
});

define_instruction!(ld_a_imm16mem, fn(emulator, _) {
    let imm16 = emulator.read_imm16_operand();
    let imm16_mem = emulator.read_address(imm16);

    emulator.regs_mut().set_a(imm16_mem);

    emulator.schedule_next_instruction(16);
});

define_instruction!(ld_imm16mem_sp, fn (emulator, _) {
    let imm16 = emulator.read_imm16_operand();
    let [low, high] = emulator.regs().sp().to_le_bytes();

    emulator.write_address(imm16, low);
    emulator.write_address(imm16 + 1, high);

    emulator.schedule_next_instruction(20);
});

define_instruction!(ldh_cmem_a, fn(emulator, _) {
    let accumulator = emulator.regs().a();
    let c = emulator.regs().c();

    emulator.write_address(ldh_address(c), accumulator);

    emulator.schedule_next_instruction(8);
});

define_instruction!(ldh_a_cmem, fn(emulator, _) {
    let c = emulator.regs().c();
    let c_mem = emulator.read_address(ldh_address(c));

    emulator.regs_mut().set_a(c_mem);

    emulator.schedule_next_instruction(8);
});

define_instruction!(ldh_imm8mem_a, fn(emulator, _) {
    let imm8 = emulator.read_imm8_operand();
    let accumulator = emulator.regs().a();

    emulator.write_address(ldh_address(imm8), accumulator);

    emulator.schedule_next_instruction(12);
});

define_instruction!(ldh_a_imm8mem, fn(emulator, _) {
    let imm8 = emulator.read_imm8_operand();
    let imm8_mem = emulator.read_address(ldh_address(imm8));

    emulator.regs_mut().set_a(imm8_mem);

    emulator.schedule_next_instruction(12);
});

define_instruction!(ld_hl_sp_imm8, fn (emulator, _) {
    let imm8_value = emulator.read_imm8_operand() as i8 as i16;
    let sp = emulator.regs().sp();

    let (result, carried) = sp.overflowing_add_signed(imm8_value);

    emulator.regs_mut().set_zero_flag(false);
    emulator.regs_mut().set_carry_flag(carried);
    emulator.regs_mut().set_subtraction_flag(false);
    emulator.regs_mut().set_half_carry_flag(half_carry_for_sp_i8_add(sp, imm8_value as i8));

    emulator.regs_mut().set_hl(result);

    emulator.schedule_next_instruction(12);
});

define_instruction!(ld_a_hli, fn (emulator, _) {
    let hl = emulator.regs().hl();
    let hl_mem = emulator.read_address(hl);

    emulator.regs_mut().set_a(hl_mem);
    emulator.regs_mut().set_hl(hl.wrapping_add(1));

    emulator.schedule_next_instruction(8);
});

define_instruction!(ld_a_hld, fn (emulator, _) {
    let hl = emulator.regs().hl();
    let hl_mem = emulator.read_address(hl);

    emulator.regs_mut().set_a(hl_mem);
    emulator.regs_mut().set_hl(hl.wrapping_sub(1));

    emulator.schedule_next_instruction(8);
});

define_instruction!(ld_hli_a, fn (emulator, _) {
    let hl = emulator.regs().hl();
    let accumulator = emulator.regs().a();

    emulator.write_address(hl, accumulator);
    emulator.regs_mut().set_hl(hl.wrapping_add(1));

    emulator.schedule_next_instruction(8);
});

define_instruction!(ld_hld_a, fn (emulator, _) {
    let hl = emulator.regs().hl();
    let accumulator = emulator.regs().a();

    emulator.write_address(hl, accumulator);
    emulator.regs_mut().set_hl(hl.wrapping_sub(1));

    emulator.schedule_next_instruction(8);
});

define_instruction!(ld_sp_hl, fn (emulator, _) {
    let hl = emulator.regs().hl();
    emulator.regs_mut().set_sp(hl);

    emulator.schedule_next_instruction(8);
});

define_instruction!(inc_r16, fn (emulator, operand) {
    let r16_operand = r16_operand(operand);
    let r16_value = emulator.read_r16_operand_value(r16_operand);

    let result = r16_value.wrapping_add(1);
    emulator.write_r16_operand_value(r16_operand, result);

    emulator.schedule_next_instruction(8);
});

define_instruction!(dec_r16, fn (emulator, operand) {
    let r16_operand = r16_operand(operand);
    let r16_value = emulator.read_r16_operand_value(r16_operand);

    let result = r16_value.wrapping_sub(1);
    emulator.write_r16_operand_value(r16_operand, result);

    emulator.schedule_next_instruction(8);
});

define_instruction!(inc_r8, fn (emulator, operand) {
    let r8_operand = high_r8_operand(operand);
    let r8_value = emulator.read_r8_operand_value(r8_operand);

    let result = r8_value.wrapping_add(1);
    emulator.write_r8_operand_value(r8_operand, result);

    // Carry flag is not set
    emulator.set_zero_flag_for_value(result);
    emulator.regs_mut().set_subtraction_flag(false);
    emulator.regs_mut().set_half_carry_flag(half_carry_for_add2(r8_value, 1));

    let num_ticks = 4 + double_r8_operand_cycles(r8_operand);
    emulator.schedule_next_instruction(num_ticks);
});

define_instruction!(dec_r8, fn (emulator, operand) {
    let r8_operand = high_r8_operand(operand);
    let r8_value = emulator.read_r8_operand_value(r8_operand);

    let result = r8_value.wrapping_sub(1);
    emulator.write_r8_operand_value(r8_operand, result);

    // Carry flag is not set
    emulator.set_zero_flag_for_value(result);
    emulator.regs_mut().set_subtraction_flag(true);
    emulator.regs_mut().set_half_carry_flag(half_carry_for_sub2(r8_value, 1));

    let num_ticks = 4 + double_r8_operand_cycles(r8_operand);
    emulator.schedule_next_instruction(num_ticks);
});

define_instruction!(add_hl_r16, fn (emulator, opcode) {
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
});

define_instruction!(rlca, fn(emulator, _) {
    // Rotate register A left, setting carry flag based on bit that was rotated around.
    let acc = emulator.regs().a();
    let high_bit = acc & 0x80;

    let rotated_acc = (acc << 1) | (high_bit >> 7);
    emulator.regs_mut().set_a(rotated_acc);

    emulator.regs_mut().set_carry_flag(high_bit != 0);
    emulator.regs_mut().set_zero_flag(false);
    emulator.regs_mut().set_bcd_flags_zero();

    emulator.schedule_next_instruction(4);
});

define_instruction!(rrca, fn(emulator, _) {
    // Rotate register A right, setting carry flag based on bit that was rotated around.
    let acc = emulator.regs().a();
    let low_bit = acc & 0x01;

    let rotated_acc = (acc >> 1) | (low_bit << 7);
    emulator.regs_mut().set_a(rotated_acc);

    emulator.regs_mut().set_carry_flag(low_bit != 0);
    emulator.regs_mut().set_zero_flag(false);
    emulator.regs_mut().set_bcd_flags_zero();

    emulator.schedule_next_instruction(4);
});

define_instruction!(rla, fn(emulator, _) {
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
});

define_instruction!(rra, fn(emulator, _) {
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
});

define_instruction!(daa, fn(emulator, _) {
    let acc = emulator.regs().a();
    let subtraction_flag = emulator.regs().subtraction_flag();
    let carry_flag = emulator.regs().carry_flag();
    let half_carry_flag = emulator.regs().half_carry_flag();

    let mut adjustment = 0x0;
    let mut carried = false;

    if half_carry_flag || (!subtraction_flag && (acc & 0x0F) > 0x09) {
        adjustment |= 0x06;
    }

    if carry_flag || (!subtraction_flag && acc > 0x99) {
        adjustment |= 0x60;
        carried = true;
    }

    let result = if subtraction_flag {
        acc.wrapping_sub(adjustment)
    } else {
        acc.wrapping_add(adjustment)
    };

    emulator.set_zero_flag_for_value(result);
    emulator.regs_mut().set_half_carry_flag(false);
    emulator.regs_mut().set_carry_flag(carried);

    emulator.schedule_next_instruction(4);
});

define_instruction!(cpl, fn(emulator, _) {
    let accumulator = emulator.regs().a();
    emulator.regs_mut().set_a(!accumulator);

    emulator.regs_mut().set_subtraction_flag(true);
    emulator.regs_mut().set_half_carry_flag(true);

    emulator.schedule_next_instruction(4);
});

define_instruction!(scf, fn(emulator, _) {
    emulator.regs_mut().set_carry_flag(true);
    emulator.regs_mut().set_bcd_flags_zero();

    emulator.schedule_next_instruction(4);
});

define_instruction!(ccf, fn (emulator, _) {
    let carry_flag = emulator.regs().carry_flag();

    emulator.regs_mut().set_carry_flag(!carry_flag);
    emulator.regs_mut().set_bcd_flags_zero();

    emulator.schedule_next_instruction(4);
});

define_instruction!(stop, fn(_, _) {
    unimplemented!("stop")
});

define_instruction!(halt, fn(emulator, _) {
    // Note that if there are pending interrupts but the IME is disabled then the CPU does not halt.
    if emulator.regs().interrupts_enabled() || emulator.interrupt_bits() == 0 {
        // emulator.halt_cpu();
    }

    emulator.schedule_next_instruction(4);
});

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

define_instruction!(add_a_r8, fn (emulator, opcode) {
    arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
        let (result, carried) = acc.overflowing_add(r8_value);

        emulator.regs_mut().set_carry_flag(carried);
        emulator.regs_mut().set_subtraction_flag(false);
        emulator.regs_mut().set_half_carry_flag(half_carry_for_add2(acc, r8_value));

        result
    });
});

define_instruction!(sub_a_r8, fn (emulator, opcode) {
    arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
        let (result, carried) = acc.overflowing_sub(r8_value);

        emulator.regs_mut().set_carry_flag(carried);
        emulator.regs_mut().set_subtraction_flag(true);
        emulator.regs_mut().set_half_carry_flag(half_carry_for_sub2(acc, r8_value));

        result
    });
});

define_instruction!(adc_a_r8, fn (emulator, opcode) {
    arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
        let carry_byte = emulator.carry_flag_byte_value();

        let (tmp, carry1) = acc.overflowing_add(r8_value);
        let (result, carry2) = tmp.overflowing_add(carry_byte);

        emulator.regs_mut().set_carry_flag(carry1 || carry2);
        emulator.regs_mut().set_subtraction_flag(false);
        emulator.regs_mut().set_half_carry_flag(half_carry_for_add3(acc, r8_value, carry_byte));

        result
    });
});

define_instruction!(sbc_a_r8, fn (emulator, opcode) {
    arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
        let carry_byte = emulator.carry_flag_byte_value();

        let (tmp, carry1) = acc.overflowing_sub(r8_value);
        let (result, carry2) = tmp.overflowing_sub(carry_byte);

        emulator.regs_mut().set_carry_flag(carry1 || carry2);
        emulator.regs_mut().set_subtraction_flag(true);
        emulator.regs_mut().set_half_carry_flag(half_carry_for_sub3(acc, r8_value, carry_byte));

        result
    });
});

define_instruction!(and_a_r8, fn (emulator, opcode) {
    arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
        emulator.regs_mut().set_carry_flag(false);
        emulator.regs_mut().set_subtraction_flag(false);
        emulator.regs_mut().set_half_carry_flag(true);

        acc & r8_value
    });
});

define_instruction!(xor_a_r8, fn (emulator, opcode) {
    arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
        emulator.regs_mut().set_carry_flag(false);
        emulator.regs_mut().set_bcd_flags_zero();

        acc ^ r8_value
    });
});

define_instruction!(or_a_r8, fn (emulator, opcode) {
    arithmetic_a_r8_instruction(emulator, opcode, |emulator, acc, r8_value| {
        emulator.regs_mut().set_carry_flag(false);
        emulator.regs_mut().set_bcd_flags_zero();

        acc | r8_value
    });
});

// Identical to sub_a_r8, but does not write the result to the accumulator.
define_instruction!(cp_a_r8, fn (emulator, opcode) {
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
        emulator.regs_mut().set_subtraction_flag(false);
        emulator.regs_mut().set_half_carry_flag(half_carry_for_add2(acc, imm8_value));

        result
    });
});

define_instruction!(sub_a_imm8, fn (emulator, _) {
    arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
        let (result, carried) = acc.overflowing_sub(imm8_value);

        emulator.regs_mut().set_carry_flag(carried);
        emulator.regs_mut().set_subtraction_flag(true);
        emulator.regs_mut().set_half_carry_flag(half_carry_for_sub2(acc, imm8_value));

        result
    });
});

define_instruction!(adc_a_imm8, fn (emulator, _) {
    arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
        let carry_byte = emulator.carry_flag_byte_value();

        let (tmp, carry1) = acc.overflowing_add(imm8_value);
        let (result, carry2) = tmp.overflowing_add(carry_byte);

        emulator.regs_mut().set_carry_flag(carry1 || carry2);
        emulator.regs_mut().set_subtraction_flag(false);
        emulator.regs_mut().set_half_carry_flag(half_carry_for_add3(acc, imm8_value, carry_byte));

        result
    });
});

define_instruction!(sbc_a_imm8, fn (emulator, _) {
    arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
        let carry_byte = emulator.carry_flag_byte_value();

        let (tmp, carry1) = acc.overflowing_sub(imm8_value);
        let (result, carry2) = tmp.overflowing_sub(carry_byte);

        emulator.regs_mut().set_carry_flag(carry1 || carry2);
        emulator.regs_mut().set_subtraction_flag(true);
        emulator.regs_mut().set_half_carry_flag(half_carry_for_sub3(acc, imm8_value, carry_byte));

        result
    });
});

define_instruction!(and_a_imm8, fn (emulator, _) {
    arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
        emulator.regs_mut().set_carry_flag(false);
        emulator.regs_mut().set_subtraction_flag(false);
        emulator.regs_mut().set_half_carry_flag(true);

        acc & imm8_value
    });
});

define_instruction!(xor_a_imm8, fn (emulator, _) {
    arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
        emulator.regs_mut().set_carry_flag(false);
        emulator.regs_mut().set_bcd_flags_zero();

        acc ^ imm8_value
    });
});

define_instruction!(or_a_imm8, fn (emulator, _) {
    arithmetic_a_imm8_instruction(emulator, |emulator, acc, imm8_value| {
        emulator.regs_mut().set_carry_flag(false);
        emulator.regs_mut().set_bcd_flags_zero();

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
    emulator.regs_mut().set_subtraction_flag(true);
    emulator.regs_mut().set_half_carry_flag(half_carry_for_sub2(acc, imm8_value));

    emulator.schedule_next_instruction(8);
});

define_instruction!(jr_imm8, fn(emulator, _) {
    let signed_offset = emulator.read_imm8_operand() as i8 as i16;
    let pc = emulator.regs().pc();

    emulator.regs_mut().set_pc(pc.wrapping_add_signed(signed_offset));

    emulator.schedule_next_instruction(12);
});

define_instruction!(jr_cc_imm8, fn(emulator, opcode) {
    let signed_offset = emulator.read_imm8_operand() as i8 as i16;
    let opcode = opcode;
    let pc = emulator.regs().pc();

    if !emulator.is_cc_met(cc_operand(opcode)) {
        emulator.schedule_next_instruction(8);
        return;
    }

    emulator.regs_mut().set_pc(pc.wrapping_add_signed(signed_offset));

    emulator.schedule_next_instruction(12);
});

define_instruction!(jp_imm16, fn (emulator, _) {
    let imm16 = emulator.read_imm16_operand();
    emulator.regs_mut().set_pc(imm16);

    emulator.schedule_next_instruction(16);
});

define_instruction!(jp_cond_imm16, fn (emulator, opcode) {
    let imm16 = emulator.read_imm16_operand();
    let cc = cc_operand(opcode);

    if !emulator.is_cc_met(cc) {
        emulator.schedule_next_instruction(12);
        return;
    }

    emulator.regs_mut().set_pc(imm16);

    emulator.schedule_next_instruction(16);
});

define_instruction!(jp_hl, fn (emulator, _) {
    let hl = emulator.regs().hl();
    emulator.regs_mut().set_pc(hl);

    emulator.schedule_next_instruction(4);
});

define_instruction!(ret, fn (emulator, _) {
    let saved_pc = emulator.pop_u16_from_stack();
    emulator.regs_mut().set_pc(saved_pc);

    emulator.schedule_next_instruction(16);
});

define_instruction!(ret_cc, fn (emulator, opcode) {
    let cc = cc_operand(opcode);

    if !emulator.is_cc_met(cc) {
        emulator.schedule_next_instruction(8);
        return;
    }

    let saved_pc = emulator.pop_u16_from_stack();
    emulator.regs_mut().set_pc(saved_pc);

    emulator.schedule_next_instruction(20);
});

define_instruction!(reti, fn (emulator, _) {
    let saved_pc = emulator.pop_u16_from_stack();
    emulator.regs_mut().set_pc(saved_pc);

    // Should immediately enable interrupts after returning
    emulator.regs_mut().set_interrupts_enabled(true);

    emulator.schedule_next_instruction(16);
});

define_instruction!(call_imm16, fn(emulator, _) {
    let imm16 = emulator.read_imm16_operand();

    // Push the current PC to the stack then set PC to the operand
    emulator.push_u16_to_stack(emulator.regs().pc());
    emulator.regs_mut().set_pc(imm16);

    emulator.schedule_next_instruction(24);
});

define_instruction!(call_cc_imm16, fn(emulator, opcode) {
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
});

define_instruction!(rst_tgt, fn(emulator, opcode) {
    let target_address = opcode & 0x38;

    // Push the current PC to the stack then set PC to the target address
    emulator.push_u16_to_stack(emulator.regs().pc());
    emulator.regs_mut().set_pc(target_address as u16);

    emulator.schedule_next_instruction(16);
});

define_instruction!(pop_r16, fn(emulator, opcode) {
    let r16_operand = r16_operand(opcode);
    let popped_value = emulator.pop_u16_from_stack();

    emulator.write_r16_operand_value(r16_operand, popped_value);

    emulator.schedule_next_instruction(12);
});

define_instruction!(pop_af, fn(emulator, _) {
    let popped_value = emulator.pop_u16_from_stack();

    emulator.regs_mut().set_af(popped_value);

    emulator.schedule_next_instruction(12);
});

define_instruction!(push_r16, fn(emulator, opcode) {
    let r16_operand = r16_operand(opcode);
    let r16_value = emulator.read_r16_operand_value(r16_operand);

    emulator.push_u16_to_stack(r16_value);

    emulator.schedule_next_instruction(16);
});

define_instruction!(push_af, fn(emulator, _) {
    let af = emulator.regs().af();
    emulator.push_u16_to_stack(af);

    emulator.schedule_next_instruction(16);
});

define_instruction!(add_sp_imm8, fn (emulator, _) {
    let signed_operand = emulator.read_imm8_operand() as i8 as i16;
    let sp = emulator.regs().sp();

    let (result, carried) = sp.overflowing_add_signed(signed_operand);
    emulator.regs_mut().set_sp(result);

    emulator.regs_mut().set_zero_flag(false);
    emulator.regs_mut().set_carry_flag(carried);
    emulator.regs_mut().set_subtraction_flag(false);
    emulator.regs_mut().set_half_carry_flag(half_carry_for_sp_i8_add(sp, signed_operand as i8));

    emulator.schedule_next_instruction(16);
});

define_instruction!(di, fn (emulator, _) {
    emulator.regs_mut().set_interrupts_enabled(false);

    emulator.schedule_next_instruction(4);
});

define_instruction!(ei, fn(emulator, _) {
    // Enable interrupts after the next instruction, so set a pending flag
    emulator.add_pending_enable_interrupts();

    emulator.schedule_next_instruction(4);
});

define_instruction!(cb_prefix, fn (emulator, _) {
    emulator.execute_cb_instruction();
});

define_instruction!(rlc, fn(emulator, opcode) {
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
});

define_instruction!(rrc, fn(emulator, opcode) {
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
});

define_instruction!(rl, fn(emulator, opcode) {
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
});

define_instruction!(rr, fn(emulator, opcode) {
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
});

define_instruction!(sla, fn(emulator, opcode) {
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
});

define_instruction!(sra, fn(emulator, opcode) {
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
});

define_instruction!(srl, fn(emulator, opcode) {
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
});

define_instruction!(swap, fn (emulator, opcode) {
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
});

define_instruction!(bit, fn (emulator, opcode) {
    let r8_operand = low_r8_operand(opcode);
    let bit_index = bit_index_operand(opcode);

    let r8_value = emulator.read_r8_operand_value(r8_operand);
    let is_bit_zero = r8_value & (1 << bit_index) == 0;

    emulator.regs_mut().set_zero_flag(is_bit_zero);
    emulator.regs_mut().set_subtraction_flag(false);
    emulator.regs_mut().set_half_carry_flag(true);

    let num_ticks = 8 + single_r8_operand_cycles(r8_operand);
    emulator.schedule_next_instruction(num_ticks);
});

define_instruction!(res, fn (emulator, opcode) {
    let r8_operand = low_r8_operand(opcode);
    let bit_index = bit_index_operand(opcode);

    // Set the bit at the given index to 0
    let r8_value = emulator.read_r8_operand_value(r8_operand);
    let result = r8_value & !(1 << bit_index);
    emulator.write_r8_operand_value(r8_operand, result);

    let num_ticks = 8 + double_r8_operand_cycles(r8_operand);
    emulator.schedule_next_instruction(num_ticks);
});

define_instruction!(set, fn (emulator, opcode) {
    let r8_operand = low_r8_operand(opcode);
    let bit_index = bit_index_operand(opcode);

    // Set the bit at the given index to 1
    let r8_value = emulator.read_r8_operand_value(r8_operand);
    let result = r8_value | (1 << bit_index);
    emulator.write_r8_operand_value(r8_operand, result);

    let num_ticks = 8 + double_r8_operand_cycles(r8_operand);
    emulator.schedule_next_instruction(num_ticks);
});

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
];

#[rustfmt::skip]
const CB_DISPATCH_TABLE: [InstructionHandler; 256] = [
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
];
