use interrupt::{Interrupt, BRK, IRQ, NMI};

use crate::{
    bus::Bus,
    mem::Mem,
    opcodes::{self, AddressingMode, OpCode, OperationKind},
};
use core::panic;

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
    pub exit_on_brk: bool,
    pub exit: bool,

    opcode: u8,
    opcode_info: Option<OpCode>,
    addr_lo: u8,
    addr_hi: u8,
    addr: u16, // Effective address
    ptr: u8,
    oper: u8, // Effective operand (value)
    /// Result of calculation which will be written to memory
    result: u8,
    state: State,
    page_crossed: bool,
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
            exit_on_brk: false,
            exit: false,
            opcode: 0,
            opcode_info: None,
            addr_lo: 0,
            addr_hi: 0,
            addr: 0,
            ptr: 0,
            oper: 0,
            result: 0,
            state: State::Next,
            page_crossed: false,
        }
    }

    fn lda(&mut self) {
        self.register_a = self.oper;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn ldx(&mut self) {
        self.register_x = self.oper;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn ldy(&mut self) {
        self.register_y = self.oper;
        self.update_zero_and_negative_flags(self.register_y);
    }

    /// LDA oper + LDX oper (M -> A -> X) \
    /// *Unofficial opecode
    fn lax(&mut self) {
        self.register_a = self.oper;
        self.register_x = self.oper;
        self.update_zero_and_negative_flags(self.oper);
    }

    fn sta(&mut self) {
        self.result = self.register_a;
    }

    fn stx(&mut self) {
        self.result = self.register_x;
    }

    fn sty(&mut self) {
        self.result = self.register_y
    }

    /// SAX oper (A & X -> oper) \
    /// *Unofficial opecode*
    fn sax(&mut self) {
        self.result = self.register_a & self.register_x;
    }

    /// CMP and DEX at once \
    /// (A AND X) - oper -> X \
    /// (A AND X) CMP oper -> (N,Z,C)
    ///
    /// AND X with A and store result in X, then subtract oper from X
    /// and set flags like CMP.
    ///
    /// *Unofficial opecode*
    fn axs(&mut self) {
        let temp_result = self.register_a & self.register_x;
        let result = temp_result.wrapping_sub(self.oper);

        self.register_x = result;

        self.status.carry_flag = temp_result >= self.oper;
        self.status.zero_flag = temp_result == self.oper;
        self.status.negative_flag = result & 0b1000_0000 != 0;
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

    fn and(&mut self) {
        self.register_a &= self.oper;
        self.update_zero_and_negative_flags(self.register_a);
    }

    /// AND oper + ROR \
    /// A AND oper, C -> \[76543210\] -> C, C = bit6, V = bit6 ^ bit5
    ///
    /// AND oper with A, then rotate one bit right in A \
    /// C is bit6, V is XOR bit6 and bit5.
    ///
    /// Affected Status flags: (N, V, Z, C)
    ///
    /// *Unofficial opcode*
    fn arr(&mut self) {
        let mut result = self.register_a & self.oper;
        result >>= 1;
        result |= (self.status.carry_flag as u8) << 7;

        self.register_a = result;

        let bit5 = (result & 0b0010_0000) >> 4 != 0;
        let bit6 = (result & 0b0100_0000) >> 5 != 0;

        self.status.carry_flag = bit6;
        self.status.overflow_flag = bit5 ^ bit6;
        self.update_zero_and_negative_flags(result);
    }

    /// AND oper + LSR \
    /// A AND oper, 0 -> [76543210] -> C \
    /// *Unofficial opcode*
    fn alr(&mut self) {
        let mut result = self.register_a & self.oper;

        let c = result & 1 != 0;
        result >>= 1;

        self.register_a = result;
        self.update_zero_and_negative_flags(result);
        self.status.carry_flag = c;
    }

    /// AND oper + set C as ASL \
    /// A AND oper, bit(7) -> C \
    /// *Unofficial opcodes*
    fn anc(&mut self) {
        self.and();
        self.status.carry_flag = self.register_a & 0x80 != 0;
    }

    /// Store X AND (high-byte of addr + 1) at addr. \
    /// X AND (H+1) -> M
    ///
    /// If page crossed (Absolute_Y), the AND operation will be droped and M will remain the same.
    ///
    /// Status flags: -
    fn shx(&mut self) {
        let rhs = (self.addr >> 8).wrapping_add(1) as u8;
        self.result = self.register_x & rhs;
    }

    /// Store Y AND (high-byte of addr + 1) at addr. \
    /// Y AND (H+1) -> M
    ///
    /// If page crossed, the AND operation will be droped and M will remain the same.
    ///
    /// Status flags: -
    fn shy(&mut self) {
        let rhs = (self.addr >> 8).wrapping_add(1) as u8;
        self.result = self.register_y & rhs;
    }

    /// MAGIC_CONSTANT AND oper -> A -> X
    ///
    /// Highly unstable, this is not accurate implementation. \
    /// See [https://www.masswerk.at/nowgobang/2021/6502-illegal-opcodes] for more detail.
    ///
    /// *Unofficial Opcode*
    fn lax_immediate(&mut self) {
        // This could be 0x00, 0xEE, etc.
        // it depends on the production of the chip, and environment conditions.
        // I'm not sure assuming it to be 0xFF is correct but it just pass test
        const MAGIC_CONST: u8 = 0xFF;

        self.register_a = MAGIC_CONST & self.oper;
        self.register_x = self.register_a;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn eor(&mut self) {
        let result = self.register_a ^ self.oper;
        self.register_a = result;
        self.update_zero_and_negative_flags(result);
    }

    fn ora(&mut self) {
        self.register_a |= self.oper;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn bit(&mut self) {
        let result = self.register_a & self.oper;
        self.status.zero_flag = result == 0;
        self.status.overflow_flag = self.oper & 0b0100_0000 != 0;
        self.status.negative_flag = self.oper & 0b1000_0000 != 0;
    }

    fn adc(&mut self) {
        let rhs = self.oper;
        let lhs = self.register_a;
        let carry_in = self.status.carry_flag as u8;

        // キャリーインとオペランドだけでオーバーフローする可能性を考慮
        // オーバーフローした場合、+0 するということになり、結果はレジスターAのままだし、キャリーフラグもそのままでいい
        // ただし、計算結果(=現状の値)に基づいて、各フラグの更新は行う
        if carry_in == 1 && rhs == 255 {
            self.status.overflow_flag = false;
            self.update_zero_and_negative_flags(self.register_a);
            return;
        }

        let (result, carry_out) = lhs.overflowing_add(rhs + carry_in);

        // キャリーフラグのオーバーフローとは別に、符号付き計算でのオーバーフローを考慮する(Vフラグ)
        let flag_v = (lhs ^ result) & (rhs ^ result) & 0x80 != 0;

        self.register_a = result;
        self.status.carry_flag = carry_out;
        self.status.overflow_flag = flag_v;
        self.update_zero_and_negative_flags(result);
    }

    fn sbc(&mut self) {
        let rhs = self.oper;
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
    }

    fn compare(&mut self, with: u8) {
        let result = with.wrapping_sub(self.oper);
        self.status.carry_flag = with >= self.oper;
        self.status.zero_flag = with == self.oper;
        self.status.negative_flag = result & 0b1000_0000 != 0;
    }

    fn inc(&mut self) {
        self.result = self.oper.wrapping_add(1);
        self.update_zero_and_negative_flags(self.result);
    }

    fn inx(&mut self) {
        self.register_x = self.register_x.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn iny(&mut self) {
        self.register_y = self.register_y.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn dec(&mut self) {
        self.result = self.oper.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.result);
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
    fn dcp(&mut self) {
        self.dec();
        //  update M
        self.oper = self.result;
        self.compare(self.register_a);
    }

    /// ISC (INC oper + SBC oper) \
    /// M + 1 -> M, A - M - !C -> A \
    /// *Unofficial opcode*
    fn isb(&mut self) {
        self.inc();
        // Update M
        self.oper = self.result;
        self.sbc();
    }

    fn asl_accumulator(&mut self) {
        let bit7 = self.register_a & 0b1000_0000 != 0;
        self.register_a <<= 1;
        self.status.carry_flag = bit7;
        self.update_zero_and_negative_flags(self.register_a);
    }

    /// Arithmetic Shift Left (ASL) \
    /// `value = value << 1`, or visually `C <- [76543210] <- 0`
    fn asl(&mut self) {
        let bit7 = self.oper & 0b1000_0000 != 0;
        self.status.carry_flag = bit7;
        self.result = self.oper << 1;
        self.update_zero_and_negative_flags(self.result);
    }

    fn lsr_accumulator(&mut self) {
        let bit0 = self.register_a & 0b0000_0001 != 0;
        self.register_a >>= 1;
        self.status.carry_flag = bit0;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn lsr(&mut self) {
        // It tooks 1 cycle to do shift operation.
        let bit0 = self.oper & 0b0000_0001 != 0;
        self.status.carry_flag = bit0;
        self.result = self.oper >> 1;
        self.update_zero_and_negative_flags(self.result);
    }

    fn rol_accumulator(&mut self) {
        let old_bit7 = self.register_a & 0b1000_0000 != 0;
        let result = (self.register_a << 1) | (self.status.carry_flag as u8);
        self.status.carry_flag = old_bit7;
        self.register_a = result;
        self.update_zero_and_negative_flags(result);
    }

    /// Rotate Left (ROL) \
    /// `value = value << 1 through C`, or visually `C <- [76543210] <- C`
    ///
    /// The value in carry is shifted into bit 0, and the bit 7 is shifted into carry.
    fn rol(&mut self) {
        let old_bit7 = self.oper & 0b1000_0000 != 0;
        self.result = (self.oper << 1) | (self.status.carry_flag as u8);
        self.status.carry_flag = old_bit7;
        self.update_zero_and_negative_flags(self.result);
    }

    fn ror_accumulator(&mut self) {
        let old_bit0 = self.register_a & 0b0000_0001 != 0;
        let result = (self.register_a >> 1) | (self.status.carry_flag as u8) << 7;
        self.register_a = result;
        self.status.carry_flag = old_bit0;
        self.update_zero_and_negative_flags(result);
    }

    fn ror(&mut self) {
        let old_bit0 = self.oper & 0b0000_0001 != 0;
        self.result = (self.oper >> 1) | (self.status.carry_flag as u8) << 7;
        self.status.carry_flag = old_bit0;
        self.update_zero_and_negative_flags(self.result);
    }

    /// ASL oper + ORA oper \
    /// M = M << 1, A OR M -> A
    fn slo(&mut self) {
        self.asl();
        self.oper = self.result;
        self.ora();
    }

    /// ROL oper + AND oper \
    /// M = M << 1 through C, A AND M -> A
    fn rla(&mut self) {
        self.rol();
        self.oper = self.result;
        self.and();
    }

    /// LSR oper + EOR oper \
    /// M = M >> 1, A EOR M -> A
    fn sre(&mut self) {
        self.lsr();
        self.oper = self.result;
        self.eor();
    }

    /// ROR oper + ADC oper \
    /// M = M >> 1 through C, A + M + C -> A
    fn rra(&mut self) {
        self.ror();
        self.oper = self.result;
        self.adc();
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
        loop {
            self.clock(&mut callback);
            if let State::Exit = self.state {
                return;
            }
        }
    }

    fn clock(&mut self, mut callback: impl FnMut(&mut CPU)) {
        match self.state {
            State::NMI(cycle) => {
                if self.interrupt(cycle, NMI) {
                    self.state = State::Done;
                } else {
                    self.state = State::NMI(cycle + 1);
                }
            }
            State::IRQ(cycle) => {
                if self.interrupt(cycle, IRQ) {
                    self.state = State::Done;
                } else {
                    self.state = State::IRQ(cycle + 1);
                }
            }
            State::Next => {
                self.addr_lo = 0;
                self.addr_hi = 0;
                self.addr = 0;
                self.ptr = 0;
                self.oper = 0;
                self.result = 0;
                self.page_crossed = false;

                callback(self);

                self.opcode = self.mem_read(self.program_counter);
                self.program_counter += 1;

                let opcode = opcodes::OPCODES_MAP
                    .get(&self.opcode)
                    .unwrap_or_else(|| panic!("OpCode {:x} is not recognized", self.opcode));
                self.opcode_info = Some(**opcode);

                self.state = State::Processing(0);
            }
            State::Processing(cycle) => {
                let info = self.opcode_info.unwrap();

                self.state = match (info.mode, info.kind) {
                    (AddressingMode::NoneAddressing, OperationKind::Other) => {
                        if self.implied(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Immediate, _) => {
                        if self.immediate(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::ZeroPage, OperationKind::Read) => {
                        if self.zero_read(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::ZeroPage, OperationKind::Mod) => {
                        if self.zero_mod(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::ZeroPage, OperationKind::Write) => {
                        if self.zero_write(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::ZeroPage_X, OperationKind::Read) => {
                        if self.zero_idx_read(cycle, Index::X) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::ZeroPage_X, OperationKind::Mod) => {
                        if self.zero_idx_mod(cycle, Index::X) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::ZeroPage_X, OperationKind::Write) => {
                        if self.zero_idx_write(cycle, Index::X) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::ZeroPage_Y, OperationKind::Read) => {
                        if self.zero_idx_read(cycle, Index::Y) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::ZeroPage_Y, OperationKind::Mod) => {
                        if self.zero_idx_mod(cycle, Index::Y) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::ZeroPage_Y, OperationKind::Write) => {
                        if self.zero_idx_write(cycle, Index::Y) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Absolute, OperationKind::Read) => {
                        if self.abs_read(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Absolute, OperationKind::Mod) => {
                        if self.abs_mod(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Absolute, OperationKind::Write) => {
                        if self.abs_write(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Absolute, OperationKind::JMP) => {
                        if self.abs_jmp(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Absolute, OperationKind::JSR) => {
                        if self.jsr(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Absolute_X, OperationKind::Read) => {
                        if self.abs_idx_read(cycle, Index::X) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Absolute_X, OperationKind::Mod) => {
                        if self.abs_idx_mod(cycle, Index::X) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Absolute_X, OperationKind::Write) => {
                        if self.abs_idx_write(cycle, Index::X) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Absolute_Y, OperationKind::Read) => {
                        if self.abs_idx_read(cycle, Index::Y) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Absolute_Y, OperationKind::Mod) => {
                        if self.abs_idx_mod(cycle, Index::Y) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Absolute_Y, OperationKind::Write) => {
                        if self.abs_idx_write(cycle, Index::Y) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Indirect, OperationKind::JMP) => {
                        if self.indirect(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Indirect_X, OperationKind::Read) => {
                        if self.indirect_x_read(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Indirect_X, OperationKind::Mod) => {
                        if self.indirect_x_mod(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Indirect_X, OperationKind::Write) => {
                        if self.indirect_x_write(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Indirect_Y, OperationKind::Read) => {
                        if self.indirect_y_read(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Indirect_Y, OperationKind::Mod) => {
                        if self.indirect_y_mod(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Indirect_Y, OperationKind::Write) => {
                        if self.indirect_y_write(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::Relative, OperationKind::Other) => {
                        if self.relative(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::NoneAddressing, OperationKind::BRK) => {
                        if self.exit_on_brk {
                            State::Exit
                        } else if self.interrupt(cycle + 1, BRK) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::NoneAddressing, OperationKind::RTI) => {
                        if self.rti(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::NoneAddressing, OperationKind::RTS) => {
                        if self.rts(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::NoneAddressing, OperationKind::PHA) => {
                        if self.pha(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::NoneAddressing, OperationKind::PHP) => {
                        if self.php(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::NoneAddressing, OperationKind::PLA) => {
                        if self.pla(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    (AddressingMode::NoneAddressing, OperationKind::PLP) => {
                        if self.plp(cycle) {
                            State::Done
                        } else {
                            State::Processing(cycle + 1)
                        }
                    }
                    _ => unreachable!(
                        "Opcode: {:02X}, Addressing Mode: {:?}, OperationKind: {:?}",
                        self.opcode,
                        self.opcode_info.unwrap().mode,
                        self.opcode_info.unwrap().kind
                    ),
                }
            }

            _ => (),
        };

        self.bus.tick(1);

        if let State::Done = self.state {
            // I'm not sure this is the right way to handle irq, but this just makes it pass test.
            // This repo also seems to use 2-step irq: https://github.com/lukexor/tetanes/blob/main/tetanes-core/src/cpu.rs#L116
            self.irq = self.irq_pending;
            self.irq_pending = self.bus.poll_irq_status();

            if self.bus.poll_nmi_status() {
                self.state = State::NMI(0)
            } else if self.irq && !self.status.interrupt_disable_flag {
                self.state = State::IRQ(0)
            } else {
                self.state = State::Next
            }
        }
    }

    // see: https://www.nesdev.org/wiki/CPU_interrupts
    fn interrupt(&mut self, cycle: u8, interrupt: interrupt::Interrupt) -> bool {
        match cycle {
            0 => {
                self.mem_read(self.program_counter);
            }
            1 => {
                self.mem_read(self.program_counter);

                if interrupt == BRK {
                    self.program_counter += 1;
                }
            }
            2 => {
                self.stack_push((self.program_counter >> 8) as u8);
            }
            3 => {
                self.stack_push(self.program_counter as u8);
            }
            4 => {
                let mut status = self.status;
                status.break_command = interrupt.break_command;
                self.stack_push(status.to_u8());
            }
            5 => {
                self.status.interrupt_disable_flag = true;
                self.program_counter = self.mem_read(interrupt.vector_addr) as u16;
            }
            6 => {
                self.program_counter |= (self.mem_read(interrupt.vector_addr + 1) as u16) << 8;
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn rti(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.mem_read(self.program_counter);
            }
            1 => (), // Increment SP
            2 => {
                self.status = Status::from_u8(self.stack_pop());
                // Increment SP
            }
            3 => {
                self.program_counter = self.stack_pop() as u16;
                // Increment SP
            }
            4 => {
                self.program_counter |= (self.stack_pop() as u16) << 8;
                return true;
            }
            _ => unreachable!(),
        }
        false
    }

    fn rts(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.mem_read(self.program_counter);
            }
            1 => {
                // Increment SP
            }
            2 => {
                self.program_counter = self.stack_pop() as u16;
                // Increment SP
            }
            3 => {
                self.program_counter |= (self.stack_pop() as u16) << 8;
            }
            4 => {
                self.program_counter += 1;
                return true;
            }
            _ => unreachable!(),
        }
        false
    }

    fn pha(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.mem_read(self.program_counter);
            }
            1 => {
                self.stack_push(self.register_a);
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn php(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.mem_read(self.program_counter);
            }
            1 => {
                let mut status = self.status;
                status.break_command = true;
                self.stack_push(status.to_u8());
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn pla(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.mem_read(self.program_counter);
            }
            1 => {
                // Increment SP
            }
            2 => {
                self.register_a = self.stack_pop();
                self.update_zero_and_negative_flags(self.register_a);
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn plp(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.mem_read(self.program_counter);
            }
            1 => {} // Increment SP
            2 => {
                self.status = Status::from_u8(self.stack_pop());
                self.status.break_command = false;
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn jsr(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.oper = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => (), // internal operation?
            2 => {
                let pch = (self.program_counter >> 8) as u8;
                self.stack_push(pch);
            }
            3 => {
                let phl = self.program_counter as u8;
                self.stack_push(phl);
            }
            4 => {
                let pcl = self.oper as u16;
                let pch = self.mem_read(self.program_counter) as u16;
                self.program_counter = pch << 8 | pcl;
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn implied(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.calculate();
                true
            }
            _ => unreachable!(),
        }
    }

    fn immediate(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.oper = self.mem_read(self.program_counter);
                self.program_counter += 1;
                self.calculate();
                true
            }
            _ => unreachable!(),
        }
    }

    fn abs_jmp(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.addr_lo = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.addr_hi = self.mem_read(self.program_counter);
                self.program_counter = (self.addr_hi as u16) << 8 | self.addr_lo as u16;
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    // Read instruction (LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT, LAX, NOP)
    fn abs_read(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.addr_lo = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.addr_hi = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            2 => {
                let addr = (self.addr_hi as u16) << 8 | self.addr_lo as u16;
                self.oper = self.mem_read(addr);
                self.calculate();
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    // Read-Modify-Write instruction
    // (ASL, LSR, ROL, ROR, INC, DEC, SLO, SRE, RLA, RRA, ISB, DCP)
    fn abs_mod(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.addr_lo = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.addr_hi = self.mem_read(self.program_counter);
                self.program_counter += 1;

                self.addr = (self.addr_hi as u16) << 8 | self.addr_lo as u16;
            }
            2 => {
                self.oper = self.mem_read(self.addr);
            }
            3 => {
                self.calculate();

                // Write the original value first.
                self.mem_write(self.addr, self.oper);
            }
            4 => {
                // Write modified value.
                self.mem_write(self.addr, self.result);
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    // Write instructions (STA, STX, STY, SAX)
    fn abs_write(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.addr_lo = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.addr_hi = self.mem_read(self.program_counter);
                self.program_counter += 1;

                self.addr = (self.addr_hi as u16) << 8 | self.addr_lo as u16;
            }
            2 => {
                self.calculate();
                self.mem_write(self.addr, self.result);
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn zero_read(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.addr_lo = self.mem_read(self.program_counter);
                self.program_counter += 1;

                self.addr = self.addr_lo as u16;
            }
            1 => {
                self.oper = self.mem_read(self.addr);
                self.calculate();
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn zero_mod(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.addr_lo = self.mem_read(self.program_counter);
                self.program_counter += 1;

                self.addr = self.addr_lo as u16;
            }
            1 => {
                self.oper = self.mem_read(self.addr);
            }
            2 => {
                self.mem_write(self.addr, self.oper);
                self.calculate();
            }
            3 => {
                self.mem_write(self.addr, self.result);
                return true;
            }
            _ => unreachable!(),
        }
        false
    }

    fn zero_write(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.addr_lo = self.mem_read(self.program_counter);
                self.program_counter += 1;

                self.addr = self.addr_lo as u16;
            }
            1 => {
                self.calculate();
                self.mem_write(self.addr, self.result);
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn zero_idx_read(&mut self, cycle: u8, idx: Index) -> bool {
        match cycle {
            0 => {
                self.addr_lo = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                // Dummy read?
                self.mem_read(self.addr_lo as u16);

                self.addr_lo = match idx {
                    Index::X => self.addr_lo.wrapping_add(self.register_x),
                    Index::Y => self.addr_lo.wrapping_add(self.register_y),
                };

                self.addr = self.addr_lo as u16;
            }
            2 => {
                self.oper = self.mem_read(self.addr);
                self.calculate();
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn zero_idx_mod(&mut self, cycle: u8, idx: Index) -> bool {
        match cycle {
            0 => {
                self.addr_lo = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.mem_read(self.addr_lo as u16);

                self.addr_lo = match idx {
                    Index::X => self.addr_lo.wrapping_add(self.register_x),
                    Index::Y => self.addr_lo.wrapping_add(self.register_y),
                };

                self.addr = self.addr_lo as u16;
            }
            2 => {
                self.oper = self.mem_read(self.addr);
            }
            3 => {
                self.mem_write(self.addr, self.oper);
                self.calculate();
            }
            4 => {
                self.mem_write(self.addr, self.result);
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn zero_idx_write(&mut self, cycle: u8, idx: Index) -> bool {
        match cycle {
            0 => {
                self.addr_lo = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.mem_read(self.addr_lo as u16);

                self.addr_lo = match idx {
                    Index::X => self.addr_lo.wrapping_add(self.register_x),
                    Index::Y => self.addr_lo.wrapping_add(self.register_y),
                };

                self.addr = self.addr_lo as u16;
            }
            2 => {
                self.calculate();
                self.mem_write(self.addr, self.result);
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn abs_idx_read(&mut self, cycle: u8, idx: Index) -> bool {
        match cycle {
            0 => {
                self.addr_lo = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.addr_hi = self.mem_read(self.program_counter);

                (self.addr_lo, self.page_crossed) = match idx {
                    Index::X => self.addr_lo.overflowing_add(self.register_x),
                    Index::Y => self.addr_lo.overflowing_add(self.register_y),
                };

                self.program_counter += 1;
            }
            2 => {
                self.oper = self.mem_read((self.addr_hi as u16) << 8 | self.addr_lo as u16);

                if self.page_crossed {
                    // Fix invalid address
                    // Next cycle will be excuted only if the address was invalid.
                    self.addr_hi = self.addr_hi.wrapping_add(1);
                } else {
                    self.calculate();
                    return true;
                }
            }
            3 => {
                self.oper = self.mem_read((self.addr_hi as u16) << 8 | self.addr_lo as u16);
                self.calculate();
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn abs_idx_mod(&mut self, cycle: u8, idx: Index) -> bool {
        match cycle {
            0 => {
                self.addr_lo = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.addr_hi = self.mem_read(self.program_counter);

                (self.addr_lo, self.page_crossed) = match idx {
                    Index::X => self.addr_lo.overflowing_add(self.register_x),
                    Index::Y => self.addr_lo.overflowing_add(self.register_y),
                };

                self.program_counter += 1;
            }
            2 => {
                self.mem_read((self.addr_hi as u16) << 8 | self.addr_lo as u16);

                if self.page_crossed {
                    self.addr_hi = self.addr_hi.wrapping_add(1);
                }

                self.addr = (self.addr_hi as u16) << 8 | self.addr_lo as u16;
            }
            3 => {
                self.oper = self.mem_read(self.addr);
            }
            4 => {
                self.mem_write(self.addr, self.oper);
                self.calculate();
            }
            5 => {
                self.mem_write(self.addr, self.result);
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn abs_idx_write(&mut self, cycle: u8, idx: Index) -> bool {
        match cycle {
            0 => {
                self.addr_lo = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.addr_hi = self.mem_read(self.program_counter);

                (self.addr_lo, self.page_crossed) = match idx {
                    Index::X => self.addr_lo.overflowing_add(self.register_x),
                    Index::Y => self.addr_lo.overflowing_add(self.register_y),
                };

                self.program_counter += 1;
            }
            2 => {
                self.mem_read((self.addr_hi as u16) << 8 | self.addr_lo as u16);

                if self.page_crossed {
                    self.addr_hi = self.addr_hi.wrapping_add(1);
                }

                self.addr = (self.addr_hi as u16) << 8 | self.addr_lo as u16;
            }
            3 => {
                self.calculate();

                // SHX, SHY will drop write operation if page boundary crossed.
                if self.page_crossed && (self.opcode == 0x9C || self.opcode == 0x9E) {
                    return true;
                }

                self.mem_write(self.addr, self.result);
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn relative(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.oper = self.mem_read(self.program_counter);
                self.program_counter += 1;

                self.calculate();

                if self.result != 1 {
                    return true;
                }
            }
            1 => {
                let mut pcl = self.program_counter as u8;
                (pcl, self.page_crossed) = pcl.overflowing_add_signed(self.oper as i8);

                // This PC will be invalid when page boundary crossed.
                // If so, Fix PC in the next cycle
                self.program_counter = (self.program_counter & 0xFF00) | pcl as u16;

                if !self.page_crossed {
                    return true;
                }
            }
            2 => {
                // Fix PCH and fetch next instruction in the next cycle.
                if (self.oper as i8) < 0 {
                    self.program_counter -= 0x0100;
                } else {
                    self.program_counter += 0x0100;
                }

                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn indirect_x_read(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.ptr = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.mem_read(self.ptr as u16);
                self.ptr = self.ptr.wrapping_add(self.register_x);
            }
            2 => {
                self.addr_lo = self.mem_read(self.ptr as u16);
            }
            3 => {
                self.addr_hi = self.mem_read(self.ptr.wrapping_add(1) as u16);
            }
            4 => {
                self.oper = self.mem_read((self.addr_hi as u16) << 8 | self.addr_lo as u16);
                self.calculate();
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn indirect_x_mod(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.ptr = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.mem_read(self.ptr as u16);
                self.ptr = self.ptr.wrapping_add(self.register_x);
            }
            2 => {
                self.addr_lo = self.mem_read(self.ptr as u16);
            }
            3 => {
                self.addr_hi = self.mem_read(self.ptr.wrapping_add(1) as u16);
                self.addr = (self.addr_hi as u16) << 8 | self.addr_lo as u16;
            }
            4 => {
                self.oper = self.mem_read(self.addr);
            }
            5 => {
                self.mem_write(self.addr, self.oper);
                self.calculate();
            }
            6 => {
                self.mem_write(self.addr, self.result);
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn indirect_x_write(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.ptr = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.mem_read(self.ptr as u16);
                self.ptr = self.ptr.wrapping_add(self.register_x);
            }
            2 => {
                self.addr_lo = self.mem_read(self.ptr as u16);
            }
            3 => {
                self.addr_hi = self.mem_read(self.ptr.wrapping_add(1) as u16);
                self.addr = (self.addr_hi as u16) << 8 | self.addr_lo as u16;
            }
            4 => {
                self.calculate();
                self.mem_write(self.addr, self.result);
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn indirect_y_read(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.ptr = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.addr_lo = self.mem_read(self.ptr as u16);
            }
            2 => {
                self.addr_hi = self.mem_read(self.ptr.wrapping_add(1) as u16);
                (self.addr_lo, self.page_crossed) = self.addr_lo.overflowing_add(self.register_y);
            }
            3 => {
                // Read from effective address before page boundary crossing is handled.
                self.oper = self.mem_read((self.addr_hi as u16) << 8 | self.addr_lo as u16);

                // Fix address if page crossed
                if self.page_crossed {
                    self.addr_hi = self.addr_hi.wrapping_add(1);
                } else {
                    self.calculate();
                    return true;
                }
            }
            4 => {
                // Read from fixed address.
                self.oper = self.mem_read((self.addr_hi as u16) << 8 | self.addr_lo as u16);
                self.calculate();
                return true;
            }

            _ => unreachable!(),
        }
        false
    }

    fn indirect_y_mod(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.ptr = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.addr_lo = self.mem_read(self.ptr as u16);
            }
            2 => {
                self.addr_hi = self.mem_read(self.ptr.wrapping_add(1) as u16);
                (self.addr_lo, self.page_crossed) = self.addr_lo.overflowing_add(self.register_y);
            }
            3 => {
                // Read from effective address before page boundary crossing is handled.
                self.oper = self.mem_read((self.addr_hi as u16) << 8 | self.addr_lo as u16);

                // Fix address if page crossed
                if self.page_crossed {
                    self.addr_hi = self.addr_hi.wrapping_add(1);
                }

                self.addr = (self.addr_hi as u16) << 8 | self.addr_lo as u16;
            }
            4 => {
                // Read from fixed address.
                self.oper = self.mem_read(self.addr);
            }
            5 => {
                self.mem_write(self.addr, self.oper);
                self.calculate();
            }
            6 => {
                self.mem_write(self.addr, self.result);
                return true;
            }
            _ => unreachable!(),
        }
        false
    }

    fn indirect_y_write(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.ptr = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.addr_lo = self.mem_read(self.ptr as u16);
            }
            2 => {
                self.addr_hi = self.mem_read(self.ptr.wrapping_add(1) as u16);
                (self.addr_lo, self.page_crossed) = self.addr_lo.overflowing_add(self.register_y);
            }
            3 => {
                // Read from effective address before page boundary crossing is handled.
                self.oper = self.mem_read((self.addr_hi as u16) << 8 | self.addr_lo as u16);

                // Fix address if page crossed
                if self.page_crossed {
                    self.addr_hi = self.addr_hi.wrapping_add(1);
                }
            }
            4 => {
                // Do operation, and set result to be written
                self.calculate();

                let addr = (self.addr_hi as u16) << 8 | self.addr_lo as u16;
                self.mem_write(addr, self.result);
                return true;
            }

            _ => unreachable!(),
        }

        false
    }

    fn indirect(&mut self, cycle: u8) -> bool {
        match cycle {
            0 => {
                self.addr_lo = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            1 => {
                self.addr_hi = self.mem_read(self.program_counter);
                self.program_counter += 1;
            }
            2 => {
                // Use ptr as a latch.
                self.ptr = self.mem_read((self.addr_hi as u16) << 8 | self.addr_lo as u16);
            }
            3 => {
                // page boundary crossing is always not handled.
                let pch = self
                    .mem_read((self.addr_hi as u16) << 8 | (self.addr_lo).wrapping_add(1) as u16);

                self.program_counter = (pch as u16) << 8 | self.ptr as u16;
                return true;
            }
            _ => unreachable!(),
        }

        false
    }

    fn calculate(&mut self) {
        match self.opcode {
            0xA9 | 0xa5 | 0xb5 | 0xad | 0xbd | 0xb9 | 0xa1 | 0xb1 => self.lda(),

            0xA2 | 0xa6 | 0xb6 | 0xae | 0xbe => self.ldx(),

            0xA0 | 0xa4 | 0xb4 | 0xac | 0xbc => self.ldy(),

            0xA7 | 0xB7 | 0xAF | 0xBF | 0xA3 | 0xB3 => self.lax(),

            0xAB => self.lax_immediate(),

            0x85 | 0x95 | 0x8d | 0x9d | 0x99 | 0x81 | 0x91 => self.sta(),

            0x86 | 0x96 | 0x8e => self.stx(),

            0x84 | 0x94 | 0x8c => self.sty(),

            0x87 | 0x97 | 0x8F | 0x83 | 0x93 => self.sax(),

            0xAA => self.tax(),
            0xa8 => self.tay(),
            0x8a => self.txa(),
            0x98 => self.tya(),

            0xba => self.tsx(),
            0x9a => self.txs(),

            0x29 | 0x25 | 0x35 | 0x2D | 0x3D | 0x39 | 0x21 | 0x31 => self.and(),

            0x6B => self.arr(),

            0x4B => self.alr(),

            0x0b | 0x2b => self.anc(),

            0x9E => self.shx(),

            0x9C => self.shy(),

            0x49 | 0x45 | 0x55 | 0x4D | 0x5D | 0x59 | 0x41 | 0x51 => self.eor(),

            0x09 | 0x05 | 0x15 | 0x0D | 0x1D | 0x19 | 0x01 | 0x11 => self.ora(),

            0x24 | 0x2C => self.bit(),

            0x69 | 0x65 | 0x75 | 0x6D | 0x7D | 0x79 | 0x61 | 0x71 => self.adc(),

            0xE9 | 0xE5 | 0xF5 | 0xED | 0xFD | 0xF9 | 0xE1 | 0xF1 | 0xEB => self.sbc(),

            0xCB => self.axs(),

            // CMP
            0xc9 | 0xc5 | 0xd5 | 0xcd | 0xdd | 0xd9 | 0xc1 | 0xd1 => self.compare(self.register_a),

            // CPX
            0xe0 | 0xe4 | 0xec => self.compare(self.register_x),

            // CPY
            0xc0 | 0xc4 | 0xcc => self.compare(self.register_y),

            0xe6 | 0xf6 | 0xee | 0xfe => self.inc(),

            0xE8 => self.inx(),

            0xC8 => self.iny(),

            0xc6 | 0xd6 | 0xce | 0xde => self.dec(),

            0xca => self.dex(),

            0x88 => self.dey(),

            0xC7 | 0xD7 | 0xCF | 0xDF | 0xDB | 0xC3 | 0xD3 => self.dcp(),

            0xE7 | 0xF7 | 0xEF | 0xFF | 0xFB | 0xE3 | 0xF3 => self.isb(),

            0x0A => self.asl_accumulator(),
            0x06 | 0x16 | 0x0E | 0x1E => self.asl(),

            0x4a => self.lsr_accumulator(),
            0x46 | 0x56 | 0x4e | 0x5e => self.lsr(),

            0x2a => self.rol_accumulator(),
            0x26 | 0x36 | 0x2e | 0x3e | 0x22 | 0x32 => self.rol(),

            0x6a => self.ror_accumulator(),
            0x66 | 0x76 | 0x6e | 0x7e | 0x62 | 0x72 => self.ror(),

            0x07 | 0x17 | 0x0F | 0x1F | 0x1B | 0x03 | 0x13 => self.slo(),

            0x27 | 0x37 | 0x2F | 0x3F | 0x3B | 0x23 | 0x33 => self.rla(),

            0x47 | 0x57 | 0x4F | 0x5F | 0x5B | 0x43 | 0x53 => self.sre(),

            0x67 | 0x77 | 0x6F | 0x7F | 0x7B | 0x63 | 0x73 => self.rra(),

            // BCC
            0x90 => self.result = !self.status.carry_flag as u8,
            // BCS
            0xB0 => self.result = self.status.carry_flag as u8,
            // BEQ
            0xF0 => self.result = self.status.zero_flag as u8,
            // BMI
            0x30 => self.result = self.status.negative_flag as u8,
            // BNE
            0xd0 => self.result = !self.status.zero_flag as u8,
            // BPL
            0x10 => self.result = !self.status.negative_flag as u8,
            // BVC
            0x50 => self.result = !self.status.overflow_flag as u8,
            // BVS
            0x70 => self.result = self.status.overflow_flag as u8,

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
                self.status.interrupt_disable_flag = true;
            }

            // NOP
            0xea | 0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => (),

            // Read NOP
            0x04 | 0x44 | 0x64 | 0x0C | 0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 | 0x80 | 0x82
            | 0x89 | 0xC2 | 0xE2 | 0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => (), // read_nop,

            _ => unimplemented!(),
        }
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
enum State {
    NMI(u8),
    IRQ(u8),
    Next,
    Processing(u8),
    Done,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Index {
    X,
    Y,
}

#[allow(clippy::upper_case_acronyms)]
mod interrupt {
    #[derive(PartialEq, Eq)]
    pub enum InterruptType {
        NMI,
        IRQ,
        BRK,
    }

    #[derive(PartialEq, Eq)]
    pub(super) struct Interrupt {
        pub(super) itype: InterruptType,
        pub(super) vector_addr: u16,
        pub(super) break_command: bool,
    }

    pub(super) const NMI: Interrupt = Interrupt {
        itype: InterruptType::NMI,
        vector_addr: 0xfffa,
        break_command: false,
    };

    pub(super) const IRQ: Interrupt = Interrupt {
        itype: InterruptType::IRQ,
        vector_addr: 0xfffe,
        break_command: false,
    };

    pub(super) const BRK: Interrupt = Interrupt {
        itype: InterruptType::BRK,
        vector_addr: 0xfffe,
        break_command: true,
    };
}

#[cfg(test)]
mod tests;
