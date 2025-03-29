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
}

impl Status {
    pub fn new() -> Self {
        Status::default()
    }

    pub fn to_u8(&self) -> u8 {
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
        }
    }
}

const STACK_BASE: u16 = 0x0100;
const STACK_RESET: u8 = 0xfd;

pub struct CPU<'a> {
    pub register_a: u8,
    pub register_x: u8,
    pub register_y: u8,
    pub status: Status,
    pub stack_pointer: u8,
    pub program_counter: u16,
    pub bus: Bus<'a>,
    log: String,
}

pub trait Mem {
    fn mem_read(&mut self, addr: u16) -> u8;

    fn mem_write(&mut self, addr: u16, data: u8);

    fn mem_read_u16(&mut self, pos: u16) -> u16 {
        let lo = self.mem_read(pos) as u16;
        let hi = self.mem_read(pos + 1) as u16;
        (hi << 8) | lo as u16
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

mod interrupt {
    #[derive(PartialEq, Eq)]
    pub enum InterruptType {
        NMI,
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
            log: String::new(),
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

        self.register_a = self.register_a | value;
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

        let rhs = rhs ^ 0b1111_1111;

        if carry_in == 1 && rhs == 255 {
            return extra_cycle as u8;
        }

        let (result, carry_out) = lhs.overflowing_add(rhs + carry_in);

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

    fn asl_accumulator(&mut self) {
        let bit7 = self.register_a & 0b1000_0000 != 0;
        self.register_a = self.register_a << 1;
        self.status.carry_flag = bit7;
        self.update_zero_and_negative_flags(self.register_a);
    }

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
        self.register_a = self.register_a >> 1;
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

    fn jmp(&mut self, mode: &AddressingMode) -> u8 {
        let mut extra_cycles = 0;
        let (addr, _) = self.get_operand_address(mode);

        if self.program_counter & 0xFF00 != addr & 0xFF00 {
            extra_cycles += 2;
        }

        self.program_counter = addr;

        return extra_cycles;
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
        let mut addnl_cycles = 0;
        if condition {
            addnl_cycles += 1;
            let offset = self.mem_read(self.program_counter) as i8;
            let jump_addr = self
                .program_counter
                .wrapping_add(1)
                .wrapping_add(offset as u16);

            if self.program_counter & 0xFF00 != jump_addr & 0xFF00 {
                addnl_cycles += 2;
            }

            self.program_counter = jump_addr;
        }

        addnl_cycles
    }

    fn rti(&mut self) {
        let data = self.stack_pop();
        self.status = Status::from_u8(data);
        self.status.break_command = false;
        self.program_counter = self.stack_pop_u16();
    }

    fn update_zero_and_negative_flags(&mut self, result: u8) {
        if result == 0 {
            self.status.zero_flag = true;
        } else {
            self.status.zero_flag = false;
        }

        if result & 0b1000_0000 != 0 {
            self.status.negative_flag = true;
        } else {
            self.status.negative_flag = false;
        }
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
    }

    pub fn run(&mut self) {
        self.run_with_callback(|_| {});
    }

    pub fn run_with_callback(&mut self, mut callback: impl FnMut(&mut CPU)) {
        let ref opcode_table: HashMap<u8, &'static opcodes::OpCode> = *opcodes::OPCODES_MAP;

        loop {
            if let Some(_nmi) = self.bus.poll_nmi_status() {
                self.interrupt(interrupt::NMI);
            }

            callback(self);

            let code = self.mem_read(self.program_counter);
            let opcode = opcode_table
                .get(&code)
                .expect(&format!("OpCode {:x} is not recognized", code));
            let mut extra_cycles = 0;

            self.log
                .push_str(&format!("{}({:x}) ", opcode.mnemonic, &opcode.code));

            self.program_counter += 1;
            let last_program_counter = self.program_counter;

            match code {
                // LDA
                0xA9 | 0xa5 | 0xb5 | 0xad | 0xbd | 0xb9 | 0xa1 | 0xb1 => {
                    extra_cycles = self.lda(&opcode.mode);
                }

                // LDX
                0xa2 | 0xa6 | 0xb6 | 0xae | 0xbe => {
                    extra_cycles = self.ldx(&opcode.mode);
                }

                // LDY
                0xa0 | 0xa4 | 0xb4 | 0xac | 0xbc => {
                    extra_cycles = self.ldy(&opcode.mode);
                }

                // STA
                0x85 | 0x95 | 0x8d | 0x9d | 0x99 | 0x81 | 0x91 => {
                    self.sta(&opcode.mode);
                }

                // STX
                0x86 | 0x96 | 0x8e => {
                    self.stx(&opcode.mode);
                }

                // STY
                0x84 | 0x94 | 0x8c => {
                    self.sty(&opcode.mode);
                }

                // TAX
                0xAA => self.tax(),
                // TAY
                0xa8 => self.tay(),
                // TXA
                0x8a => self.txa(),
                // TYA
                0x98 => self.tya(),

                // TSX
                0xba => self.tsx(),
                // TXS
                0x9a => self.txs(),
                // PHA
                0x48 => self.pha(),
                // PHP
                0x08 => self.php(),
                // PLA
                0x68 => self.pla(),
                // PLP
                0x28 => self.plp(),

                // AND
                0x29 | 0x25 | 0x35 | 0x2D | 0x3D | 0x39 | 0x21 | 0x31 => {
                    extra_cycles = self.and(&opcode.mode);
                }

                // EOR
                0x49 | 0x45 | 0x55 | 0x4D | 0x5D | 0x59 | 0x41 | 0x51 => {
                    extra_cycles = self.eor(&opcode.mode);
                }

                // ORA
                0x09 | 0x05 | 0x15 | 0x0D | 0x1D | 0x19 | 0x01 | 0x11 => {
                    extra_cycles = self.ora(&opcode.mode);
                }

                // BIT
                0x24 | 0x2C => {
                    self.bit(&opcode.mode);
                }

                // ADC
                0x69 | 0x65 | 0x75 | 0x6D | 0x7D | 0x79 | 0x61 | 0x71 => {
                    extra_cycles = self.adc(&opcode.mode);
                }

                // SBC
                0xE9 | 0xE5 | 0xF5 | 0xED | 0xFD | 0xF9 | 0xE1 | 0xF1 => {
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

                // INC
                0xe6 | 0xf6 | 0xee | 0xfe => {
                    self.inc(&opcode.mode);
                }

                // INX
                0xE8 => self.inx(),

                // INY
                0xC8 => self.iny(),

                // DEC
                0xc6 | 0xd6 | 0xce | 0xde => {
                    self.dec(&opcode.mode);
                }

                // DEX
                0xca => self.dex(),

                // DEY
                0x88 => self.dey(),

                // ASL accumulator
                0x0A => {
                    self.asl_accumulator();
                }
                // ASL
                0x06 | 0x16 | 0x0E | 0x1E => {
                    self.asl(&opcode.mode);
                }

                // LSR accumulator
                0x4a => {
                    self.lsr_accumulator();
                }
                // LSR
                0x46 | 0x56 | 0x4e | 0x5e => {
                    self.lsr(&opcode.mode);
                }

                // ROL accumulator
                0x2a => self.rol_accumulator(),
                // ROL
                0x26 | 0x36 | 0x2e | 0x3e | 0x22 | 0x32 => {
                    self.rol(&opcode.mode);
                }

                // ROR accumulator
                0x6a => self.ror_accumulator(),
                // ROR
                0x66 | 0x76 | 0x6e | 0x7e | 0x62 | 0x72 => {
                    self.ror(&opcode.mode);
                }

                // JMP
                0x4c | 0x6c => {
                    extra_cycles = self.jmp(&opcode.mode);
                }

                // JSR
                0x20 => {
                    self.jsr(&opcode.mode);
                }

                // RTS
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

                // BRK
                0x00 => return,
                // NOP
                0xea => (),
                // RTI
                0x40 => self.rti(),

                _ => todo!(),
            }

            self.bus.tick(opcode.cycles + extra_cycles);

            // If not jump or branch occured
            if last_program_counter == self.program_counter {
                self.program_counter += (opcode.len - 1) as u16;
            }
        }
    }

    fn interrupt(&mut self, interrupt: interrupt::Interrupt) {
        self.stack_push_u16(self.program_counter);
        let mut flag = self.status.clone();
        flag.break_command = interrupt.b_flag_mask & 0b0001_0000 != 0;

        self.stack_push(flag.to_u8());
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

                let ptr: u8 = (base as u8).wrapping_add(self.register_x);
                let lo = self.mem_read(ptr as u16);
                let hi = self.mem_read(ptr.wrapping_add(1) as u16);
                ((hi as u16) << 8 | (lo as u16), false)
            }
            AddressingMode::Indirect_Y => {
                let base: u8 = self.mem_read(self.program_counter);
                let lo = self.mem_read(base as u16);
                let hi = self.mem_read((base as u8).wrapping_add(1) as u16);
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
mod test {
    use std::vec;

    use crate::rom::test::TestRom;

    use super::*;

    #[test]
    fn test_0xa9_lda_immediate_load_data() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0x05, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();
        assert_eq!(cpu.register_a, 0x05);
        assert!(cpu.status.to_u8() & 0b000_0010 == 0b00);
        assert!(cpu.status.to_u8() & 0b1000_0000 == 0);
    }

    #[test]
    fn test_0xa9_lda_zero_frag() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0x00, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();
        assert!(cpu.status.to_u8() & 0b000_0010 == 0b10);
    }

    #[test]
    fn test_0xaa_tax_move_a_to_x() {
        let test_rom = TestRom::create_test_rom(vec![0xaa, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.register_a = 10;
        cpu.run();

        assert_eq!(cpu.register_x, 10)
    }

    #[test]
    fn test_inx_overflow() {
        let test_rom = TestRom::create_test_rom(vec![0xe8, 0xe8, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.register_x = 0xff;
        cpu.run();

        assert_eq!(cpu.register_x, 1)
    }

    #[test]
    fn test_5_ops_working_togather() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0xc0, 0xaa, 0xe8, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_x, 0xc1)
    }

    #[test]
    fn test_lda_from_memory() {
        let test_rom = TestRom::create_test_rom(vec![0xa5, 0x10, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.mem_write(0x10, 0x55);
        cpu.run();

        assert_eq!(cpu.register_a, 0x55);
    }

    #[test]
    fn test_status_to_u8() {
        let mut status = Status::from_u8(0b0010_0100);
        status.zero_flag = true;

        assert_eq!(status.to_u8(), 0b0010_0110);

        status.carry_flag = true;
        status.negative_flag = true;

        assert_eq!(status.to_u8(), 0b1010_0111);
    }

    #[test]
    fn test_adc() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0xc0, 0x69, 0xc4, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_a, 0x84);
        assert_eq!(cpu.status.negative_flag, true);
        assert_eq!(cpu.status.overflow_flag, false);
        assert_eq!(cpu.status.carry_flag, true);
    }

    #[test]
    fn test_adc2() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0x50, 0x69, 0x50, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_a, 0xa0);
        assert_eq!(cpu.status.negative_flag, true);
        assert_eq!(cpu.status.overflow_flag, true);
        assert_eq!(cpu.status.carry_flag, false);
    }

    #[test]
    fn test_adc3() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0xd0, 0x69, 0x90, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_a, 0x60);
        assert_eq!(cpu.status.negative_flag, false);
        assert_eq!(cpu.status.overflow_flag, true);
        assert_eq!(cpu.status.carry_flag, true);
    }

    #[test]
    fn test_adc_ff() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0x7f, 0x69, 0x7f, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.status.carry_flag = true;
        cpu.run();

        assert_eq!(cpu.register_a, 0xff);
        assert_eq!(cpu.status.negative_flag, true);
        assert_eq!(cpu.status.overflow_flag, true);
        assert_eq!(cpu.status.carry_flag, false);
    }

    #[test]
    fn test_adc_carry_in() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0x50, 0x69, 0x10, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.status.carry_flag = true;

        cpu.run();

        assert_eq!(cpu.register_a, 0x61);
        assert_eq!(cpu.status.negative_flag, false);
        assert_eq!(cpu.status.overflow_flag, false);
        assert_eq!(cpu.status.carry_flag, false);
    }

    #[test]
    fn test_sbc() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0x50, 0xE9, 0xf0, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_a, 0x5f);
        assert_eq!(cpu.status.negative_flag, false);
        assert_eq!(cpu.status.overflow_flag, false);
        assert_eq!(cpu.status.carry_flag, false);
    }

    #[test]
    fn test_sbc2() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0x50, 0xe9, 0xb0, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_a, 0x9f);
        assert_eq!(cpu.status.negative_flag, true);
        assert_eq!(cpu.status.overflow_flag, true);
        assert_eq!(cpu.status.carry_flag, false);
    }

    #[test]
    fn test_sbc3() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0xd0, 0xe9, 0x70, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_a, 0x5f);
        assert_eq!(cpu.status.negative_flag, false);
        assert_eq!(cpu.status.overflow_flag, true);
        assert_eq!(cpu.status.carry_flag, true);
    }

    #[test]
    fn test_and() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0b1010_1010, 0x29, 0b0101_0101, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_a, 0);

        let test_rom = TestRom::create_test_rom(vec![0xa9, 0b1010_1010, 0x29, 0b0101_1010, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_a, 0b0000_1010);
    }

    #[test]
    fn test_asl_accumulator() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0x50, 0x0a, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_a, 0xa0);
        assert_eq!(cpu.status.negative_flag, true);
        assert_eq!(cpu.status.zero_flag, false);
        assert_eq!(cpu.status.carry_flag, false);

        let test_rom = TestRom::create_test_rom(vec![0xa9, 0xf0, 0x0a, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_a, 0xe0);
        assert_eq!(cpu.status.negative_flag, true);
        assert_eq!(cpu.status.zero_flag, false);
        assert_eq!(cpu.status.carry_flag, true);
    }

    #[test]
    fn test_asl() {
        let test_rom =
            TestRom::create_test_rom(vec![0xa9, 0b1010_1010, 0x85, 0xc0, 0x06, 0xc0, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.mem_read(0xc0), 0b0101_0100);
        assert_eq!(cpu.status.negative_flag, false);
        assert_eq!(cpu.status.zero_flag, false);
        assert_eq!(cpu.status.carry_flag, true);
    }

    #[test]
    fn test_bcc() {
        let test_rom = TestRom::create_test_rom(vec![0x90, 0x01, 0x00, 0xa9, 0x51, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_a, 0x51);
    }

    #[test]
    fn test_bcc2() {
        let test_rom =
            TestRom::create_test_rom(vec![0x90, 0x04, 0x00, 0xa9, 0x51, 0x00, 0x90, 0xFB, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_a, 0x51);
    }

    #[test]
    fn test_beq() {
        let test_rom = TestRom::create_test_rom(vec![0xf0, 0x01, 0x00, 0xa9, 0x51, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.status.zero_flag = true;
        cpu.run();

        assert_eq!(cpu.register_a, 0x51);
    }

    #[test]
    fn test_bmi() {
        let test_rom = TestRom::create_test_rom(vec![0x30, 0x01, 0x00, 0xa9, 0x51, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.status.negative_flag = true;
        cpu.run();

        assert_eq!(cpu.register_a, 0x51);
    }

    #[test]
    fn test_bit() {
        let test_rom = TestRom::create_test_rom(vec![
            0xa9,
            0b1010_1010,
            0x85,
            0xc0,
            0xa9,
            0b1011_1111,
            0x24,
            0xc0,
            0x00,
        ]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.status.zero_flag, false);
        assert_eq!(cpu.status.overflow_flag, false);
        assert_eq!(cpu.status.negative_flag, true);
    }

    #[test]
    fn test_bit2() {
        let test_rom = TestRom::create_test_rom(vec![
            0xa9,
            0b0011_1111,
            0x85,
            0xc0,
            0xa9,
            0b1100_0000,
            0x24,
            0xc0,
            0x00,
        ]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.status.zero_flag, true);
        assert_eq!(cpu.status.overflow_flag, false);
        assert_eq!(cpu.status.negative_flag, false);
    }

    #[test]
    fn test_bne() {
        let test_rom = TestRom::create_test_rom(vec![0xd0, 0x01, 0x00, 0xa9, 0x51, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.status.zero_flag = false;
        cpu.run();

        assert_eq!(cpu.register_a, 0x51);
    }

    #[test]
    fn test_bpl() {
        let test_rom = TestRom::create_test_rom(vec![0x10, 0x01, 0x00, 0xa9, 0x51, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.status.negative_flag = false;
        cpu.run();

        assert_eq!(cpu.register_a, 0x51);
    }

    #[test]
    fn test_bvc() {
        let test_rom = TestRom::create_test_rom(vec![0x50, 0x01, 0x00, 0xa9, 0x51, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.status.overflow_flag = false;
        cpu.run();

        assert_eq!(cpu.register_a, 0x51);
    }

    #[test]
    fn test_bvs() {
        let test_rom = TestRom::create_test_rom(vec![0x70, 0x01, 0x00, 0xa9, 0x51, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.status.overflow_flag = true;
        cpu.run();

        assert_eq!(cpu.register_a, 0x51);
    }

    #[test]
    fn test_cmp() {
        let test_rom = TestRom::create_test_rom(vec![0xc9, 0x51, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.register_a = 0x51;
        cpu.run();

        assert_eq!(cpu.status.carry_flag, true);
        assert_eq!(cpu.status.zero_flag, true);
        assert_eq!(cpu.status.negative_flag, false);
    }

    #[test]
    fn test_cpx() {
        let test_rom = TestRom::create_test_rom(vec![0xe0, 0x51, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.register_x = 0x51;
        cpu.run();

        assert_eq!(cpu.status.carry_flag, true);
        assert_eq!(cpu.status.zero_flag, true);
        assert_eq!(cpu.status.negative_flag, false);
    }

    #[test]
    fn test_cpy() {
        let test_rom = TestRom::create_test_rom(vec![0xc0, 0x51, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.register_y = 0x51;
        cpu.run();

        assert_eq!(cpu.status.carry_flag, true);
        assert_eq!(cpu.status.zero_flag, true);
        assert_eq!(cpu.status.negative_flag, false);
    }

    #[test]
    fn test_dec() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0x51, 0x85, 0xc0, 0xc6, 0xc0, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.mem_read(0xc0), 0x50);
        assert_eq!(cpu.status.negative_flag, false);
        assert_eq!(cpu.status.zero_flag, false);
    }

    #[test]
    fn test_dex() {
        let test_rom = TestRom::create_test_rom(vec![0xca, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.register_x = 0x51;
        cpu.run();

        assert_eq!(cpu.register_x, 0x50);
        assert_eq!(cpu.status.negative_flag, false);
        assert_eq!(cpu.status.zero_flag, false);
    }

    #[test]
    fn test_dey() {
        let test_rom = TestRom::create_test_rom(vec![0x88, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.register_y = 0x51;
        cpu.run();

        assert_eq!(cpu.register_y, 0x50);
        assert_eq!(cpu.status.negative_flag, false);
        assert_eq!(cpu.status.zero_flag, false);
    }

    #[test]
    fn test_eor() {
        let test_rom = TestRom::create_test_rom(vec![0x49, 0x51, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.register_a = 0x51;
        cpu.run();

        assert_eq!(cpu.register_a, 0x00);
        assert_eq!(cpu.status.negative_flag, false);
        assert_eq!(cpu.status.zero_flag, true);
    }

    #[test]
    fn test_inc() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0x51, 0x85, 0xc0, 0xe6, 0xc0, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.mem_read(0xc0), 0x52);
        assert_eq!(cpu.status.negative_flag, false);
        assert_eq!(cpu.status.zero_flag, false);
    }

    #[test]
    fn test_iny() {
        let test_rom = TestRom::create_test_rom(vec![0xc8, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.register_y = 0x51;
        cpu.run();

        assert_eq!(cpu.register_y, 0x52);
        assert_eq!(cpu.status.negative_flag, false);
        assert_eq!(cpu.status.zero_flag, false);
    }

    #[test]
    fn test_jmp() {
        let test_rom = TestRom::create_test_rom(vec![
            0xa9, 0x01, 0x85, 0xf0, 0xa9, 0xcc, 0x85, 0xf1, 0x6c, 0xf0, 0x00,
        ]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.program_counter, 0xcc02);
    }

    #[test]
    fn test_jsr() {
        let test_rom = TestRom::create_test_rom(vec![0x20, 0x03, 0x80, 0xa9, 0x51, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_a, 0x51);
        assert_eq!(cpu.stack_pointer, STACK_RESET - 2);
        assert_eq!(
            cpu.mem_read_u16(STACK_BASE + (cpu.stack_pointer as u16) + 1),
            0x8000 + 2
        );
    }

    #[test]
    fn test_rts() {
        let test_rom =
            TestRom::create_test_rom(vec![0x20, 0x04, 0x80, 0x00, 0xa9, 0x51, 0x60, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.stack_pointer, STACK_RESET);
        assert_eq!(cpu.program_counter, 0x8004);
    }

    #[test]
    fn test_ldx() {
        let test_rom = TestRom::create_test_rom(vec![0xa2, 0x51, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_x, 0x51);
    }

    #[test]
    fn test_ldy() {
        let test_rom = TestRom::create_test_rom(vec![0xa0, 0x51, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_y, 0x51);
    }

    #[test]
    fn test_lsr_accumulator() {
        let test_rom = TestRom::create_test_rom(vec![0x4a, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.register_a = 0b0101_0101;
        cpu.run();

        assert_eq!(cpu.register_a, 0b0010_1010);
        assert_eq!(cpu.status.carry_flag, true);
    }

    #[test]
    fn test_ora() {
        let test_rom = TestRom::create_test_rom(vec![0x09, 0b0101_0101, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.register_a = 0b1010_1010;
        cpu.run();

        assert_eq!(cpu.register_a, 0b1111_1111);
    }

    #[test]
    fn test_pha() {
        let test_rom = TestRom::create_test_rom(vec![0x48, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.register_a = 0x51;
        cpu.run();

        assert_eq!(cpu.stack_pop(), 0x51);
    }

    #[test]
    fn test_php() {
        let test_rom = TestRom::create_test_rom(vec![0x08, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.status.carry_flag = true;
        cpu.run();

        assert_eq!(cpu.stack_pop(), 0b0011_0101);
    }

    #[test]
    fn test_pla() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0x51, 0x48, 0xa9, 0x50, 0x68, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_a, 0x51);
    }

    #[test]
    fn test_plp() {
        let test_rom = TestRom::create_test_rom(vec![0x08, 0x38, 0x28, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.status.carry_flag, false);
    }

    #[test]
    fn test_rol_accumulator() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0b1010_1010, 0x2a, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.status.carry_flag = true;
        cpu.run();

        assert_eq!(cpu.register_a, 0b0101_0101);
        assert_eq!(cpu.status.carry_flag, true);
        assert_eq!(cpu.status.negative_flag, false);
        assert_eq!(cpu.status.zero_flag, false);
    }

    #[test]
    fn test_rol() {
        let test_rom =
            TestRom::create_test_rom(vec![0xa9, 0b1010_1010, 0x85, 0xc0, 0x26, 0xc0, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.status.carry_flag = true;
        cpu.run();

        assert_eq!(cpu.mem_read(0xc0), 0b0101_0101);
        assert_eq!(cpu.status.carry_flag, true);
        assert_eq!(cpu.status.negative_flag, false);
        assert_eq!(cpu.status.zero_flag, false);
    }

    #[test]
    fn test_ror_accumulator() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0b1010_1010, 0x6a, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.status.carry_flag = true;
        cpu.run();

        assert_eq!(cpu.register_a, 0b1101_0101);
        assert_eq!(cpu.status.carry_flag, false);
        assert_eq!(cpu.status.negative_flag, true);
        assert_eq!(cpu.status.zero_flag, false);
    }

    #[test]
    fn test_ror() {
        let test_rom =
            TestRom::create_test_rom(vec![0xa9, 0b1010_1010, 0x85, 0xc0, 0x66, 0xc0, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.status.carry_flag = true;
        cpu.run();

        assert_eq!(cpu.mem_read(0xc0), 0b1101_0101);
        assert_eq!(cpu.status.carry_flag, false);
        assert_eq!(cpu.status.negative_flag, true);
        assert_eq!(cpu.status.zero_flag, false);
    }

    #[test]
    fn test_stx() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0x51, 0xaa, 0x86, 0xc0, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.mem_read(0xc0), 0x51);
    }

    #[test]
    fn test_sty() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0x51, 0xa8, 0x84, 0xc0, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.mem_read(0xc0), 0x51);
    }

    #[test]
    fn test_tay() {
        let test_rom = TestRom::create_test_rom(vec![0xa9, 0x51, 0xa8, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_y, 0x51);
    }

    #[test]
    fn test_tsx() {
        let test_rom = TestRom::create_test_rom(vec![0xba, 0x00]);
        let mut cpu = CPU::new(Bus::new(test_rom, |_, _| {}));
        cpu.reset();
        cpu.run();

        assert_eq!(cpu.register_x, 0xfd);
    }
}
