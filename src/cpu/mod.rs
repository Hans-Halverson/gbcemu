use crate::emulator::Emulator;

pub mod registers;

impl Emulator {
    /// Execute an instruction, returning the number of clock cycles taken by the instruction.
    pub fn execute_instruction(&mut self) -> usize {
        let opcode = self.advance_pc();
        DISPATCH_TABLE[opcode as usize](self, opcode)
    }

    /// Read the byte at PC and advance the PC by 1.
    fn advance_pc(&mut self) -> u8 {
        let pc = self.regs().pc();
        let byte = self.read_address(pc);
        self.regs_mut().set_pc(pc + 1);
        byte
    }
}

macro_rules! unimplemented_instruction {
    ($name:ident) => {
        fn $name(_: &mut Emulator, _: Opcode) -> usize {
            unimplemented!(stringify!($name));
        }
    };
}

type Opcode = u8;
type InstructionHandler = fn(&mut Emulator, Opcode) -> usize;

// Each M cycle takes 4 clock cycles
const M_CYCLE_LENGTH: usize = 4;
const ONE_M_CYCLE: usize = M_CYCLE_LENGTH;

fn nop(_: &mut Emulator, _: Opcode) -> usize {
    // Do nothing
    ONE_M_CYCLE
}

unimplemented_instruction!(ld_r16_imm16);

unimplemented_instruction!(ld_r16mem_a);

unimplemented_instruction!(ld_a_r16mem);

unimplemented_instruction!(ld_imm16mem_sp);

unimplemented_instruction!(inc_r16);
unimplemented_instruction!(dec_r16);
unimplemented_instruction!(inc_r8);
unimplemented_instruction!(dec_r8);

unimplemented_instruction!(add_hl_r16);

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

unimplemented_instruction!(add_a_r8);
unimplemented_instruction!(adc_a_r8);
unimplemented_instruction!(sub_a_r8);
unimplemented_instruction!(sbc_a_r8);
unimplemented_instruction!(and_a_r8);
unimplemented_instruction!(xor_a_r8);
unimplemented_instruction!(or_a_r8);
unimplemented_instruction!(cp_a_r8);

unimplemented_instruction!(add_a_imm8);
unimplemented_instruction!(adc_a_imm8);
unimplemented_instruction!(sub_a_imm8);
unimplemented_instruction!(sbc_a_imm8);
unimplemented_instruction!(and_a_imm8);
unimplemented_instruction!(xor_a_imm8);
unimplemented_instruction!(or_a_imm8);
unimplemented_instruction!(cp_a_imm8);

unimplemented_instruction!(ret_cc);
unimplemented_instruction!(ret);
unimplemented_instruction!(reti);
unimplemented_instruction!(jp_cond_imm16);
unimplemented_instruction!(jp_imm16);
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

unimplemented_instruction!(add_sp_imm8);
unimplemented_instruction!(ld_hl_sp_imm8);
unimplemented_instruction!(ld_sp_hl);

unimplemented_instruction!(di);
unimplemented_instruction!(ei);

/// An opcode that does not match any valid instruction.
fn invalid(_: &mut Emulator, opcode: Opcode) -> usize {
    panic!("Invalid opcode {:02X}", opcode);
}

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
