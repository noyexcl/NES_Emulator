use tracing::trace;

use crate::{bus::Bus, opcodes};
use core::panic;
use std::collections::HashMap;

/// # Status Register (P) http://wiki.nesdev.com/w/index.php/Status_flags
/// # unused flag(5) is always 1 because it's hardwired so.
///
///  7 6 5 4 3 2 1 0
///  N V _ B D I Z C
///  | |   | | | | +--- Carry Flag
///  | |   | | | +----- Zero Flag
///  | |   | | +------- Interrupt Disable
///  | |   | +--------- Decimal Mode (not used on NES)
///  | |   +----------- Break Command
///  | +--------------- Overflow Flag
///  +----------------- Negative Flag
#[derive(Copy, Clone, Debug, Default)]
pub struct Status {
    pub carry_flag: bool,
    pub zero_flag: bool,
    pub interrupt_disable_flag: bool,
    pub decimal_mode_flag: bool, // not implemented in ricoh's 6502
    pub break_command: bool,
    pub overflow_flag: bool,
    pub negative_flag: bool,

    /// IRQ inhibition triggered by CLI, SEI and PLP does not affect immediately, it's delayd by 1 instruction.
    /// On the other hand RTI affects the irq flag immediately.
    irq_disable_pending: Option<bool>,
}

impl Status {
    pub fn to_u8(self) -> u8 {
        (self.negative_flag as u8) << 7
            | (self.overflow_flag as u8) << 6
            | 1 << 5
            | (self.break_command as u8) << 4
            | (self.decimal_mode_flag as u8) << 3
            | (self.interrupt_disable_flag as u8) << 2
            | (self.zero_flag as u8) << 1
            | (self.carry_flag as u8)
    }

    pub fn from_u8(data: u8) -> Self {
        Status {
            negative_flag: (data & 0b1000_0000) != 0,
            overflow_flag: (data & 0b0100_0000) != 0,
            break_command: (data & 0b0001_0000) != 0,
            decimal_mode_flag: (data & 0b0000_1000) != 0,
            interrupt_disable_flag: (data & 0b0000_0100) != 0,
            zero_flag: (data & 0b0000_0010) != 0,
            carry_flag: (data & 0b0000_0001) != 0,
            irq_disable_pending: None,
        }
    }
}

const STACK_BASE: u16 = 0x0100;
const STACK_RESET: u8 = 0xfd;

#[allow(clippy::upper_case_acronyms)]
pub struct CPU<'a> {
    pub register_a: u8,
    pub register_x: u8,
    pub register_y: u8,
    pub status: Status,
    pub stack_pointer: u8,
    pub program_counter: u16,
    pub bus: Bus<'a>,
    pub irq: bool,
    pub irq_pending: bool,
}

pub trait Mem {
    fn mem_read(&mut self, addr: u16) -> u8;

    fn mem_write(&mut self, addr: u16, data: u8);

    fn mem_read_u16(&mut self, pos: u16) -> u16 {
        let lo = self.mem_read(pos) as u16;
        let hi = self.mem_read(pos + 1) as u16;
        (hi << 8) | lo
    }

    fn mem_write_u16(&mut self, pos: u16, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.mem_write(pos, lo);
        self.mem_write(pos + 1, hi);
    }
}

impl Mem for CPU<'_> {
    fn mem_read(&mut self, addr: u16) -> u8 {
        self.bus.mem_read(addr)
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        self.bus.mem_write(addr, data)
    }

    fn mem_read_u16(&mut self, pos: u16) -> u16 {
        self.bus.mem_read_u16(pos)
    }

    fn mem_write_u16(&mut self, pos: u16, data: u16) {
        self.bus.mem_write_u16(pos, data)
    }
}

#[allow(clippy::upper_case_acronyms)]
mod interrupt {
    #[derive(PartialEq, Eq)]
    pub enum InterruptType {
        NMI,
        IRQ,
    }

    #[derive(PartialEq, Eq)]
    pub(super) struct Interrupt {
        pub(super) itype: InterruptType,
        pub(super) vector_addr: u16,
        pub(super) b_flag_mask: u8,
        pub(super) cpu_cycles: u8,
    }

    pub(super) const NMI: Interrupt = Interrupt {
        itype: InterruptType::NMI,
        vector_addr: 0xfffa,
        b_flag_mask: 0b0010_0000,
        cpu_cycles: 2,
    };

    pub(super) const IRQ: Interrupt = Interrupt {
        itype: InterruptType::IRQ,
        vector_addr: 0xfffe,
        b_flag_mask: 0,
        cpu_cycles: 2,
    };
}

impl<'a> CPU<'a> {
    pub fn new(bus: Bus<'a>) -> Self {
        CPU {
            register_a: 0,
            register_x: 0,
            register_y: 0,
            status: Status::default(),
            stack_pointer: 0,
            program_counter: 0,
            bus,
            irq: false,
            irq_pending: false,
        }
    }

    fn lda(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, extra_cycle) = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        self.register_a = value;
        self.update_zero_and_negative_flags(self.register_a);
        extra_cycle as u8
    }

    fn ldx(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, extra_cycle) = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        self.register_x = value;
        self.update_zero_and_negative_flags(self.register_x);
        extra_cycle as u8
    }

    fn ldy(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, extra_cycle) = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        self.register_y = value;
        self.update_zero_and_negative_flags(self.register_y);
        extra_cycle as u8
    }

    /// LDA oper + LDX oper (M -> A -> X) \
    /// *Unofficial opecode
    fn lax(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, extra_cycle) = self.get_operand_address(mode);
        let value = self.mem_read(addr);
        self.register_a = value;

        self.register_x = value;
        self.update_zero_and_negative_flags(value);
        extra_cycle as u8
    }

    fn sta(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        self.mem_write(addr, self.register_a);
    }

    fn stx(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        self.mem_write(addr, self.register_x);
    }

    fn sty(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        self.mem_write(addr, self.register_y);
    }

    /// SAX oper (A & X -> oper) \
    /// *Unofficial opecode
    fn sax(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        let value = self.register_a & self.register_x;
        self.mem_write(addr, value);
    }

    fn tax(&mut self) {
        self.register_x = self.register_a;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn tay(&mut self) {
        self.register_y = self.register_a;
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn txa(&mut self) {
        self.register_a = self.register_x;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn tya(&mut self) {
        self.register_a = self.register_y;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn tsx(&mut self) {
        self.register_x = self.stack_pointer;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn txs(&mut self) {
        self.stack_pointer = self.register_x;
    }

    fn pha(&mut self) {
        self.stack_push(self.register_a);
    }

    fn php(&mut self) {
        let mut status = self.status;
        status.break_command = true;
        self.stack_push(status.to_u8());
    }

    fn pla(&mut self) {
        self.register_a = self.stack_pop();
        self.update_zero_and_negative_flags(self.register_a);
    }

    /// SP = SP + 1 \
    /// NVxxDIZC = (STACK_BASE + SP)
    ///
    /// Pop value from stack, and set it as status flags. \
    /// If there is a change of irq disable flag, it's delayed by 1 instruction.
    fn plp(&mut self) {
        self.status = Status::from_u8(self.stack_pop());
        self.status.break_command = false;
    }

    fn and(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, extra_cycle) = self.get_operand_address(mode);
        let rhs = self.mem_read(addr);

        let result = self.register_a & rhs;
        self.register_a = result;
        self.update_zero_and_negative_flags(result);

        extra_cycle as u8
    }

    fn eor(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, extra_cycle) = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        let result = self.register_a ^ value;
        self.register_a = result;
        self.update_zero_and_negative_flags(result);

        extra_cycle as u8
    }

    fn ora(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, extra_cycle) = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        self.register_a |= value;
        self.update_zero_and_negative_flags(self.register_a);

        extra_cycle as u8
    }

    fn bit(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        let result = self.register_a & value;

        self.status.zero_flag = result == 0;
        self.status.overflow_flag = value & 0b0100_0000 != 0;
        self.status.negative_flag = value & 0b1000_0000 != 0;
    }

    fn adc(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, extra_cycle) = self.get_operand_address(mode);

        let rhs = self.mem_read(addr);
        let lhs = self.register_a;
        let carry_in = self.status.carry_flag as u8;

        // キャリーインとオペランドだけでオーバーフローする可能性を考慮
        // もしオーバーフローしたならば、結果はレジスターAのままだし、キャリービットもそもままでいいから何もしなくても良いはず
        if carry_in == 1 && rhs == 255 {
            return extra_cycle as u8;
        }

        let (result, carry_out) = lhs.overflowing_add(rhs + carry_in);

        // キャリーフラグのオーバーフローとは別に、符号付き計算でのオーバーフローを考慮する(Vフラグ)
        let flag_v = (lhs ^ result) & (rhs ^ result) & 0x80 != 0;

        self.register_a = result;
        self.status.carry_flag = carry_out;
        self.status.overflow_flag = flag_v;
        self.update_zero_and_negative_flags(result);

        extra_cycle as u8
    }

    fn sbc(&mut self, mode: &AddressingMode) -> u8 {
        let (addr, extra_cycle) = self.get_operand_address(mode);
        let rhs = self.mem_read(addr);
        let lhs = self.register_a;
        let carry_in = self.status.carry_flag as u8;

        let rhs = !rhs;
        let (temp_result, carry_out1) = lhs.overflowing_add(rhs);
        let (result, carry_out2) = temp_result.overflowing_add(carry_in);
        let carry_out = carry_out1 || carry_out2;

        let flag_v = (lhs ^ result) & (rhs ^ result) & 0x80 != 0;

        self.register_a = result;
        self.status.carry_flag = carry_out;
        self.status.overflow_flag = flag_v;
        self.update_zero_and_negative_flags(result);

        extra_cycle as u8
    }

    fn compare(&mut self, mode: &AddressingMode, with: u8) -> u8 {
        let (addr, extra_cycle) = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        let result = with.wrapping_sub(value);
        self.status.carry_flag = with >= value;
        self.status.zero_flag = with == value;
        self.status.negative_flag = result & 0b1000_0000 != 0;

        extra_cycle as u8
    }

    fn inc(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        let result = value.wrapping_add(1);
        self.mem_write(addr, result);
        self.update_zero_and_negative_flags(result);
    }

    fn inx(&mut self) {
        self.register_x = self.register_x.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn iny(&mut self) {
        self.register_y = self.register_y.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn dec(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        let result = value.wrapping_sub(1);
        self.mem_write(addr, result);
        self.update_zero_and_negative_flags(result);
    }

    fn dex(&mut self) {
        self.register_x = self.register_x.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn dey(&mut self) {
        self.register_y = self.register_y.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_y);
    }

    /// DCP (DEC oper + CMP oper) \
    /// M - 1 -> M, A - M \
    /// *Unofficial opcode*
    fn dcp(&mut self, mode: &AddressingMode) {
        self.dec(mode);
        self.compare(mode, self.register_a);
    }

    /// ISC (INC oper + SBC oper) \
    /// M + 1 -> M, A - M - !C -> A \
    /// *Unofficial opcode*
    fn isb(&mut self, mode: &AddressingMode) {
        self.inc(mode);
        self.sbc(mode);
    }

    fn asl_accumulator(&mut self) {
        let bit7 = self.register_a & 0b1000_0000 != 0;
        self.register_a <<= 1;
        self.status.carry_flag = bit7;
        self.update_zero_and_negative_flags(self.register_a);
    }

    /// Arithmetic Shift Left (ASL) \
    /// `value = value << 1`, or visually `C <- [76543210] <- 0`
    fn asl(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        let bit7 = value & 0b1000_0000 != 0;
        let result = value << 1;
        self.mem_write(addr, result);
        self.status.carry_flag = bit7;
        self.update_zero_and_negative_flags(result);
    }

    fn lsr_accumulator(&mut self) {
        let bit0 = self.register_a & 0b0000_0001 != 0;
        self.register_a >>= 1;
        self.status.carry_flag = bit0;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn lsr(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        let bit0 = value & 0b0000_0001 != 0;
        let result = value >> 1;
        self.mem_write(addr, result);
        self.status.carry_flag = bit0;
        self.update_zero_and_negative_flags(result);
    }

    fn rol_accumulator(&mut self) {
        let old_bit7 = self.register_a & 0b1000_0000 != 0;
        self.register_a = (self.register_a << 1) | (self.status.carry_flag as u8);
        self.status.carry_flag = old_bit7;
        self.update_zero_and_negative_flags(self.register_a);
    }

    /// Rotate Left (ROL) \
    /// `value = value << 1 through C`, or visually `C <- [76543210] <- C`
    ///
    /// The value in carry is shifted into bit 0, and the bit 7 is shifted into carry.
    fn rol(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        let old_bit7 = value & 0b1000_0000 != 0;
        let result = (value << 1) | (self.status.carry_flag as u8);
        self.mem_write(addr, result);
        self.status.carry_flag = old_bit7;
        self.update_zero_and_negative_flags(result);
    }

    fn ror_accumulator(&mut self) {
        let old_bit0 = self.register_a & 0b0000_0001 != 0;
        self.register_a = (self.register_a >> 1) | (self.status.carry_flag as u8) << 7;
        self.status.carry_flag = old_bit0;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn ror(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        let value = self.mem_read(addr);

        let old_bit0 = value & 0b0000_0001 != 0;
        let result = (value >> 1) | (self.status.carry_flag as u8) << 7;
        self.mem_write(addr, result);
        self.status.carry_flag = old_bit0;
        self.update_zero_and_negative_flags(result);
    }

    /// ASL oper + ORA oper \
    /// M = M << 1, A OR M -> A
    fn slo(&mut self, mode: &AddressingMode) {
        self.asl(mode);
        self.ora(mode);
    }

    /// ROL oper + AND oper \
    /// M = M << 1 through C, A AND M -> A
    fn rla(&mut self, mode: &AddressingMode) {
        self.rol(mode);
        self.and(mode);
    }

    /// LSR oper + EOR oper \
    /// M = M >> 1, A EOR M -> A
    fn sre(&mut self, mode: &AddressingMode) {
        self.lsr(mode);
        self.eor(mode);
    }

    /// ROR oper + ADC oper \
    /// M = M >> 1 through C, A + M + C -> A
    fn rra(&mut self, mode: &AddressingMode) {
        self.ror(mode);
        self.adc(mode);
    }

    fn jmp(&mut self, mode: &AddressingMode) {
        let (addr, _) = self.get_operand_address(mode);
        self.program_counter = addr;
    }

    fn jsr(&mut self, mode: &AddressingMode) {
        let (jump_addr, _) = self.get_operand_address(mode);
        self.stack_push_u16(self.program_counter + 2 - 1);
        self.program_counter = jump_addr;
    }

    fn rts(&mut self) {
        let addr = self.stack_pop_u16();
        self.program_counter = addr.wrapping_add(1);
    }

    fn branch(&mut self, condition: bool) -> u8 {
        let mut extra_cycles = 0;
        if condition {
            extra_cycles += 1;
            let offset = self.mem_read(self.program_counter) as i8;
            let jump_addr = self
                .program_counter
                .wrapping_add(1)
                .wrapping_add_signed(offset as i16);

            if self.program_counter.wrapping_add(1) & 0xFF00 != jump_addr & 0xFF00 {
                extra_cycles += 1;
            }

            self.program_counter = jump_addr;
        }

        extra_cycles
    }

    fn rti(&mut self) {
        let data = self.stack_pop();
        self.status = Status::from_u8(data);
        self.status.break_command = false;
        self.program_counter = self.stack_pop_u16();
    }

    // Read but do nothing. Return extra cycle if the address crossed pages
    fn read_nop(&mut self, mode: &AddressingMode) -> u8 {
        let (_, page_crossed) = self.get_operand_address(mode);
        page_crossed as u8
    }

    fn update_zero_and_negative_flags(&mut self, result: u8) {
        self.status.zero_flag = result == 0;
        self.status.negative_flag = result & 0b1000_0000 != 0;
    }

    fn stack_push(&mut self, data: u8) {
        self.mem_write(STACK_BASE + self.stack_pointer as u16, data);
        self.stack_pointer = self.stack_pointer.wrapping_sub(1);
    }

    fn stack_pop(&mut self) -> u8 {
        self.stack_pointer = self.stack_pointer.wrapping_add(1);
        self.mem_read(STACK_BASE + self.stack_pointer as u16)
    }

    fn stack_push_u16(&mut self, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;

        self.stack_push(hi);
        self.stack_push(lo);
    }

    fn stack_pop_u16(&mut self) -> u16 {
        let lo = self.stack_pop() as u16;
        let hi = self.stack_pop() as u16;

        hi << 8 | lo
    }

    pub fn reset(&mut self) {
        self.register_a = 0;
        self.register_x = 0;
        self.status = Status::from_u8(0b0010_0100);
        self.stack_pointer = STACK_RESET;
        self.program_counter = self.mem_read_u16(0xFFFC);
        self.bus.reset();
    }

    pub fn run(&mut self) {
        self.run_with_callback(|_| {});
    }

    pub fn run_with_callback(&mut self, mut callback: impl FnMut(&mut CPU)) {
        let opcode_table: &HashMap<u8, &'static opcodes::OpCode> = &opcodes::OPCODES_MAP;

        loop {
            if self.bus.cpu_stall > 0 {
                // Skip CPU process if stalled
                let c = self.bus.cpu_stall;
                self.bus.cpu_stall = 0;
                self.bus.tick(c as u8);
                continue;
            }

            if let Some(_nmi) = self.bus.poll_nmi_status() {
                self.interrupt(interrupt::NMI);
            } else if self.irq && !self.status.interrupt_disable_flag {
                self.interrupt(interrupt::IRQ);
            }

            callback(self);

            let code = self.mem_read(self.program_counter);
            let opcode = opcode_table
                .get(&code)
                .unwrap_or_else(|| panic!("OpCode {:x} is not recognized", code));
            let mut extra_cycles = 0;

            self.program_counter += 1;
            let last_program_counter = self.program_counter;

            match code {
                0xA9 | 0xa5 | 0xb5 | 0xad | 0xbd | 0xb9 | 0xa1 | 0xb1 => {
                    extra_cycles = self.lda(&opcode.mode);
                }

                0xa2 | 0xa6 | 0xb6 | 0xae | 0xbe => {
                    extra_cycles = self.ldx(&opcode.mode);
                }

                0xa0 | 0xa4 | 0xb4 | 0xac | 0xbc => {
                    extra_cycles = self.ldy(&opcode.mode);
                }

                0xA7 | 0xB7 | 0xAF | 0xBF | 0xA3 | 0xB3 => {
                    extra_cycles = self.lax(&opcode.mode);
                }

                0x85 | 0x95 | 0x8d | 0x9d | 0x99 | 0x81 | 0x91 => {
                    self.sta(&opcode.mode);
                }

                0x86 | 0x96 | 0x8e => {
                    self.stx(&opcode.mode);
                }

                0x84 | 0x94 | 0x8c => {
                    self.sty(&opcode.mode);
                }

                0x87 | 0x97 | 0x8F | 0x83 | 0x93 => {
                    self.sax(&opcode.mode);
                }

                0xAA => self.tax(),
                0xa8 => self.tay(),
                0x8a => self.txa(),
                0x98 => self.tya(),

                0xba => self.tsx(),
                0x9a => self.txs(),

                0x48 => self.pha(),
                0x08 => self.php(),
                0x68 => self.pla(),
                0x28 => self.plp(),

                0x29 | 0x25 | 0x35 | 0x2D | 0x3D | 0x39 | 0x21 | 0x31 => {
                    extra_cycles = self.and(&opcode.mode);
                }

                0x49 | 0x45 | 0x55 | 0x4D | 0x5D | 0x59 | 0x41 | 0x51 => {
                    extra_cycles = self.eor(&opcode.mode);
                }

                0x09 | 0x05 | 0x15 | 0x0D | 0x1D | 0x19 | 0x01 | 0x11 => {
                    extra_cycles = self.ora(&opcode.mode);
                }

                0x24 | 0x2C => {
                    self.bit(&opcode.mode);
                }

                0x69 | 0x65 | 0x75 | 0x6D | 0x7D | 0x79 | 0x61 | 0x71 => {
                    extra_cycles = self.adc(&opcode.mode);
                }

                0xE9 | 0xE5 | 0xF5 | 0xED | 0xFD | 0xF9 | 0xE1 | 0xF1 | 0xEB => {
                    extra_cycles = self.sbc(&opcode.mode);
                }

                // CMP
                0xc9 | 0xc5 | 0xd5 | 0xcd | 0xdd | 0xd9 | 0xc1 | 0xd1 => {
                    extra_cycles = self.compare(&opcode.mode, self.register_a);
                }

                // CPX
                0xe0 | 0xe4 | 0xec => {
                    extra_cycles = self.compare(&opcode.mode, self.register_x);
                }

                // CPY
                0xc0 | 0xc4 | 0xcc => {
                    extra_cycles = self.compare(&opcode.mode, self.register_y);
                }

                0xe6 | 0xf6 | 0xee | 0xfe => {
                    self.inc(&opcode.mode);
                }

                0xE8 => self.inx(),

                0xC8 => self.iny(),

                0xc6 | 0xd6 | 0xce | 0xde => {
                    self.dec(&opcode.mode);
                }

                0xca => self.dex(),

                0x88 => self.dey(),

                0xC7 | 0xD7 | 0xCF | 0xDF | 0xDB | 0xC3 | 0xD3 => {
                    self.dcp(&opcode.mode);
                }

                0xE7 | 0xF7 | 0xEF | 0xFF | 0xFB | 0xE3 | 0xF3 => {
                    self.isb(&opcode.mode);
                }

                0x0A => {
                    self.asl_accumulator();
                }
                0x06 | 0x16 | 0x0E | 0x1E => {
                    self.asl(&opcode.mode);
                }

                0x4a => {
                    self.lsr_accumulator();
                }
                0x46 | 0x56 | 0x4e | 0x5e => {
                    self.lsr(&opcode.mode);
                }

                0x2a => self.rol_accumulator(),
                0x26 | 0x36 | 0x2e | 0x3e | 0x22 | 0x32 => {
                    self.rol(&opcode.mode);
                }

                0x6a => self.ror_accumulator(),
                0x66 | 0x76 | 0x6e | 0x7e | 0x62 | 0x72 => {
                    self.ror(&opcode.mode);
                }

                0x07 | 0x17 | 0x0F | 0x1F | 0x1B | 0x03 | 0x13 => {
                    self.slo(&opcode.mode);
                }

                0x27 | 0x37 | 0x2F | 0x3F | 0x3B | 0x23 | 0x33 => {
                    self.rla(&opcode.mode);
                }

                0x47 | 0x57 | 0x4F | 0x5F | 0x5B | 0x43 | 0x53 => {
                    self.sre(&opcode.mode);
                }

                0x67 | 0x77 | 0x6F | 0x7F | 0x7B | 0x63 | 0x73 => {
                    self.rra(&opcode.mode);
                }

                0x4c | 0x6c => {
                    self.jmp(&opcode.mode);
                }

                0x20 => {
                    self.jsr(&opcode.mode);
                }

                0x60 => {
                    self.rts();
                }

                // BCC
                0x90 => {
                    extra_cycles = self.branch(!self.status.carry_flag);
                }
                // BCS
                0xB0 => {
                    extra_cycles = self.branch(self.status.carry_flag);
                }
                // BEQ
                0xF0 => {
                    extra_cycles = self.branch(self.status.zero_flag);
                }
                // BMI
                0x30 => {
                    extra_cycles = self.branch(self.status.negative_flag);
                }
                // BNE
                0xd0 => {
                    extra_cycles = self.branch(!self.status.zero_flag);
                }
                // BPL
                0x10 => {
                    extra_cycles = self.branch(!self.status.negative_flag);
                }
                // BVC
                0x50 => {
                    extra_cycles = self.branch(!self.status.overflow_flag);
                }
                // BVS
                0x70 => {
                    extra_cycles = self.branch(self.status.overflow_flag);
                }

                // CLC
                0x18 => {
                    self.status.carry_flag = false;
                }
                0xd8 => {
                    self.status.decimal_mode_flag = false;
                }
                // CLI
                0x58 => {
                    // TODO: Delay flag change by 1 instruction
                    self.status.interrupt_disable_flag = false;
                }
                // CLV
                0xB8 => {
                    self.status.overflow_flag = false;
                }

                // SEC
                0x38 => {
                    self.status.carry_flag = true;
                }
                // SED
                0xF8 => {
                    self.status.decimal_mode_flag = true;
                }
                // SEI
                0x78 => {
                    // TODO: Delay flag change by instruction
                    self.status.interrupt_disable_flag = true;
                }

                // BRK
                0x00 => return,

                // NOP
                0xea | 0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA | 0x04 | 0x44 | 0x64 | 0x0C
                | 0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 | 0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => (),

                0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => {
                    extra_cycles += self.read_nop(&opcode.mode);
                }

                0x40 => self.rti(),

                _ => todo!(),
            }

            // I'm not sure this is the right way to handle irq, but this just makes it pass test.
            // This repo also seems to use 2-step irq: https://github.com/lukexor/tetanes/blob/main/tetanes-core/src/cpu.rs#L116
            self.irq = self.irq_pending;
            self.irq_pending = self.bus.poll_irq_status();

            self.bus.tick(opcode.cycles + extra_cycles);

            // If not jump or branch occured
            if last_program_counter == self.program_counter {
                self.program_counter += (opcode.len - 1) as u16;
            }
        }
    }

    fn interrupt(&mut self, interrupt: interrupt::Interrupt) {
        self.stack_push_u16(self.program_counter);
        let mut status = self.status;
        status.break_command = interrupt.b_flag_mask & 0b0010_0000 != 0;

        self.stack_push(status.to_u8());
        self.status.interrupt_disable_flag = true;

        self.bus.tick(interrupt.cpu_cycles);
        self.program_counter = self.mem_read_u16(interrupt.vector_addr);
    }

    /// (Address, Whether page crossed)
    fn get_operand_address(&mut self, mode: &AddressingMode) -> (u16, bool) {
        match mode {
            AddressingMode::Immediate => (self.program_counter, false),
            AddressingMode::ZeroPage => (self.mem_read(self.program_counter) as u16, false),
            AddressingMode::Absolute => (self.mem_read_u16(self.program_counter), false),

            AddressingMode::ZeroPage_X => {
                let pos = self.mem_read(self.program_counter);
                let addr = pos.wrapping_add(self.register_x) as u16;
                (addr, false)
            }
            AddressingMode::ZeroPage_Y => {
                let pos = self.mem_read(self.program_counter);
                let addr = pos.wrapping_add(self.register_y) as u16;
                (addr, false)
            }
            AddressingMode::Absolute_X => {
                let base = self.mem_read_u16(self.program_counter);
                let addr = base.wrapping_add(self.register_x as u16);
                (addr, page_crossed(base, addr))
            }
            AddressingMode::Absolute_Y => {
                let base = self.mem_read_u16(self.program_counter);
                let addr = base.wrapping_add(self.register_y as u16);
                (addr, page_crossed(base, addr))
            }
            AddressingMode::Indirect => {
                // 6502にはページをまたぐIndirectにバグが存在している
                // 例えば、JMP ($30FF) という命令の場合、
                // 本来は、$30FFにある値(下位バイト)と$3100(上位バイト)にある値を参照しなければならないが
                // $30FF(下位バイト)と$3000(上位バイト)の値を参照してしまう
                // ここではそれを再現している
                let addr = self.mem_read_u16(self.program_counter);

                // 対象のアドレスがFFで終わる場合、つまりページをまたぐ場合はバグを再現
                if addr & 0x00FF == 0x00FF {
                    let lo = self.mem_read(addr);
                    let hi = self.mem_read(addr & 0xFF00);
                    ((hi as u16) << 8 | (lo as u16), false)
                } else {
                    (self.mem_read_u16(addr), false)
                }
            }
            AddressingMode::Indirect_X => {
                let base = self.mem_read(self.program_counter);

                let ptr: u8 = base.wrapping_add(self.register_x);
                let lo = self.mem_read(ptr as u16);
                let hi = self.mem_read(ptr.wrapping_add(1) as u16);
                ((hi as u16) << 8 | (lo as u16), false)
            }
            AddressingMode::Indirect_Y => {
                let base: u8 = self.mem_read(self.program_counter);
                let lo = self.mem_read(base as u16);
                let hi = self.mem_read(base.wrapping_add(1) as u16);
                let deref_base = (hi as u16) << 8 | (lo as u16);
                let deref = deref_base.wrapping_add(self.register_y as u16);
                (deref, page_crossed(deref, deref_base))
            }
            AddressingMode::NoneAddressing => {
                panic!("mode {:?} is not supported", mode);
            }
        }
    }
}

fn page_crossed(addr1: u16, addr2: u16) -> bool {
    addr1 & 0xFF00 != addr2 & 0xFF00
}

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum AddressingMode {
    Immediate,
    ZeroPage,
    ZeroPage_X,
    ZeroPage_Y,
    Absolute,
    Absolute_X,
    Absolute_Y,
    Indirect,
    Indirect_X,
    Indirect_Y,
    NoneAddressing,
}

#[cfg(test)]
mod tests;
