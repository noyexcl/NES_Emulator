use super::cpu::AddressingMode;
use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    pub static ref CPU_OPS_CODES: Vec<OpCode> = vec![
    // more info at https://www.nesdev.org/obelisk-6502-guide/instructions.html

    /* --- Load Operations --- */
    // Load Accumulator (N,Z)
    OpCode::new(0xa9, "LDA", 2, 2, AddressingMode::Immediate),
    OpCode::new(0xa5, "LDA", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0xb5, "LDA", 2, 4, AddressingMode::ZeroPage_X),
    OpCode::new(0xad, "LDA", 3, 4, AddressingMode::Absolute),
    OpCode::new(0xbd, "LDA", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_X),
    OpCode::new(0xb9, "LDA", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_Y),
    OpCode::new(0xa1, "LDA", 2, 6, AddressingMode::Indirect_X),
    OpCode::new(0xb1, "LDA", 2, 5, /* +1 if page crossed */ AddressingMode::Indirect_Y),
    // Load X Register (N,Z)
    OpCode::new(0xa2, "LDX", 2, 2, AddressingMode::Immediate),
    OpCode::new(0xa6, "LDX", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0xb6, "LDX", 2, 4, AddressingMode::ZeroPage_Y),
    OpCode::new(0xae, "LDX", 3, 4, AddressingMode::Absolute),
    OpCode::new(0xbe, "LDX", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_Y),
    // Load Y Register (N,Z)
    OpCode::new(0xa0, "LDY", 2, 2, AddressingMode::Immediate),
    OpCode::new(0xa4, "LDY", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0xb4, "LDY", 2, 4, AddressingMode::ZeroPage_X),
    OpCode::new(0xac, "LDY", 3, 4, AddressingMode::Absolute),
    OpCode::new(0xbc, "LDY", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_X),

    /* --- Store Operation --- */
    // Store accumulator
    OpCode::new(0x85, "STA", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0x95, "STA", 2, 4, AddressingMode::ZeroPage_X),
    OpCode::new(0x8d, "STA", 3, 4, AddressingMode::Absolute),
    OpCode::new(0x9d, "STA", 3, 5, AddressingMode::Absolute_X),
    OpCode::new(0x99, "STA", 3, 5, AddressingMode::Absolute_Y),
    OpCode::new(0x81, "STA", 2, 6, AddressingMode::Indirect_X),
    OpCode::new(0x91, "STA", 2, 6, AddressingMode::Indirect_Y),
    // Store X register
    OpCode::new(0x86, "STX", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0x96, "STX", 2, 4, AddressingMode::ZeroPage_Y),
    OpCode::new(0x8e, "STX", 3, 4, AddressingMode::Absolute),
    // Store Y register
    OpCode::new(0x84, "STY", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0x94, "STY", 2, 4, AddressingMode::ZeroPage_X),
    OpCode::new(0x8c, "STY", 3, 4, AddressingMode::Absolute),

    /* --- Register Transfers --- */
    // Transfer accumulator to X (N,Z)
    OpCode::new(0xaa, "TAX", 1, 2, AddressingMode::NoneAddressing),
    // Transfer accumulator to Y (N,Z)
    OpCode::new(0xa8, "TAY", 1, 2, AddressingMode::NoneAddressing),
    // Transfer X to accumulator (N,Z)
    OpCode::new(0x8a, "TXA", 1, 2, AddressingMode::NoneAddressing),
    // Transfer Y to accumulator (N,Z)
    OpCode::new(0x98, "TYA", 1, 2, AddressingMode::NoneAddressing),

    /* --- Stack Operations --- */
    // Transfer stack pointer to X (N,Z)
    OpCode::new(0xba, "TSX", 1, 2, AddressingMode::NoneAddressing),
    // Transfer X to stack pointer
    OpCode::new(0x9a, "TXS", 1, 2, AddressingMode::NoneAddressing),
    // Push accumulator on stack
    OpCode::new(0x48, "PHA", 1, 3, AddressingMode::NoneAddressing),
    // Push processor status on stack
    OpCode::new(0x08, "PHP", 1, 3, AddressingMode::NoneAddressing),
    // Pull accumulator from stack (N,Z)
    OpCode::new(0x68, "PLA", 1, 4, AddressingMode::NoneAddressing),
    // Pull processor status from stack (All)
    OpCode::new(0x28, "PLP", 1, 4, AddressingMode::NoneAddressing),

    /* --- Logical --- */
    // Logical AND (N,Z)
    OpCode::new(0x29, "AND", 2, 2, AddressingMode::Immediate),
    OpCode::new(0x25, "AND", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0x35, "AND", 2, 4, AddressingMode::ZeroPage_X),
    OpCode::new(0x2D, "AND", 3, 4, AddressingMode::Absolute),
    OpCode::new(0x3D, "AND", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_X),
    OpCode::new(0x39, "AND", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_Y),
    OpCode::new(0x21, "AND", 2, 6, AddressingMode::Indirect_X),
    OpCode::new(0x31, "AND", 2, 5, /* +1 if page crossed */ AddressingMode::Indirect_Y),
    // Exclusive OR (N,Z)
    OpCode::new(0x49, "EOR", 2, 2, AddressingMode::Immediate),
    OpCode::new(0x45, "EOR", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0x55, "EOR", 2, 4, AddressingMode::ZeroPage_X),
    OpCode::new(0x4d, "EOR", 3, 4, AddressingMode::Absolute),
    OpCode::new(0x5d, "EOR", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_X),
    OpCode::new(0x59, "EOR", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_Y),
    OpCode::new(0x41, "EOR", 2, 6, AddressingMode::Indirect_X),
    OpCode::new(0x51, "EOR", 2, 5, /* +1 if page crossed */ AddressingMode::Indirect_Y),
    // Logical Inclusive OR (N,Z)
    OpCode::new(0x09, "ORA", 2, 2, AddressingMode::Immediate),
    OpCode::new(0x05, "ORA", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0x15, "ORA", 2, 4, AddressingMode::ZeroPage_X),
    OpCode::new(0x0d, "ORA", 3, 4, AddressingMode::Absolute),
    OpCode::new(0x1d, "ORA", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_X),
    OpCode::new(0x19, "ORA", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_Y),
    OpCode::new(0x01, "ORA", 2, 6, AddressingMode::Indirect_X),
    OpCode::new(0x11, "ORA", 2, 5, /* +1 if page crossed */ AddressingMode::Indirect_Y),
    // Bit Test
    OpCode::new(0x24, "BIT", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0x2C, "BIT", 3, 4, AddressingMode::Absolute),

    /* --- Arithmetic --- */
    // Add with Carry (N,V,Z,C)
    OpCode::new(0x69, "ADC", 2, 2, AddressingMode::Immediate),
    OpCode::new(0x65, "ADC", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0x75, "ADC", 2, 4, AddressingMode::ZeroPage_X),
    OpCode::new(0x6D, "ADC", 3, 4, AddressingMode::Absolute),
    OpCode::new(0x7D, "ADC", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_X),
    OpCode::new(0x79, "ADC", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_Y),
    OpCode::new(0x61, "ADC", 2, 6, AddressingMode::Indirect_X),
    OpCode::new(0x71, "ADC", 2, 5, /* +1 if page crossed */ AddressingMode::Indirect_Y),
    // Subtract with Carry (N,V,Z,C)
    OpCode::new(0xE9, "SBC", 2, 2, AddressingMode::Immediate),
    OpCode::new(0xE5, "SBC", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0xF5, "SBC", 2, 4, AddressingMode::ZeroPage_X),
    OpCode::new(0xED, "SBC", 3, 4, AddressingMode::Absolute),
    OpCode::new(0xFD, "SBC", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_X),
    OpCode::new(0xF9, "SBC", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_Y),
    OpCode::new(0xE1, "SBC", 2, 6, AddressingMode::Indirect_X),
    OpCode::new(0xF1, "SBC", 2, 5, /* +1 if page crossed */ AddressingMode::Indirect_Y),
    // Compare accumulator (N,Z,C)
    OpCode::new(0xC9, "CMP", 2, 2, AddressingMode::Immediate),
    OpCode::new(0xC5, "CMP", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0xD5, "CMP", 2, 4, AddressingMode::ZeroPage_X),
    OpCode::new(0xCD, "CMP", 3, 4, AddressingMode::Absolute),
    OpCode::new(0xDD, "CMP", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_X),
    OpCode::new(0xD9, "CMP", 3, 4, /* +1 if page crossed */ AddressingMode::Absolute_Y),
    OpCode::new(0xC1, "CMP", 2, 6, AddressingMode::Indirect_X),
    OpCode::new(0xD1, "CMP", 2, 5, /* +1 if page crossed */ AddressingMode::Indirect_Y),
    // Compare X register (N,Z,C)
    OpCode::new(0xE0, "CPX", 2, 2, AddressingMode::Immediate),
    OpCode::new(0xE4, "CPX", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0xEC, "CPX", 3, 4, AddressingMode::Absolute),
    // Compare Y register (N,Z,C)
    OpCode::new(0xC0, "CPY", 2, 2, AddressingMode::Immediate),
    OpCode::new(0xC4, "CPY", 2, 3, AddressingMode::ZeroPage),
    OpCode::new(0xCC, "CPY", 3, 4, AddressingMode::Absolute),

    /* --- Increments & Decrements --- */
    // Increment a memory location (N,Z)
    OpCode::new(0xe6, "INC", 2, 5, AddressingMode::ZeroPage),
    OpCode::new(0xf6, "INC", 2, 6, AddressingMode::ZeroPage_X),
    OpCode::new(0xee, "INC", 3, 6, AddressingMode::Absolute),
    OpCode::new(0xfe, "INC", 3, 7, AddressingMode::Absolute_X),
    // Increment X register (N,Z)
    OpCode::new(0xe8, "INX", 1, 2, AddressingMode::NoneAddressing),
    // Increment Y register (N,Z)
    OpCode::new(0xc8, "INY", 1, 2, AddressingMode::NoneAddressing),
    // Decrement a memory location (N,Z)
    OpCode::new(0xc6, "DEC", 2, 5, AddressingMode::ZeroPage),
    OpCode::new(0xd6, "DEC", 2, 6, AddressingMode::ZeroPage_X),
    OpCode::new(0xce, "DEC", 3, 6, AddressingMode::Absolute),
    OpCode::new(0xde, "DEC", 3, 7, AddressingMode::Absolute_X),
    // Decrement X register (N,Z)
    OpCode::new(0xca, "DEX", 1, 2, AddressingMode::NoneAddressing),
    // Decrement Y register (N,Z)
    OpCode::new(0x88, "DEY", 1, 2, AddressingMode::NoneAddressing),

    /* --- Shifts --- */
    // Arithmetic Shift Left (N,Z,C)
    OpCode::new(0x0A, "ASL", 1, 2, AddressingMode::NoneAddressing),
    OpCode::new(0x06, "ASL", 2, 5, AddressingMode::ZeroPage),
    OpCode::new(0x16, "ASL", 2, 6, AddressingMode::ZeroPage_X),
    OpCode::new(0x0E, "ASL", 3, 6, AddressingMode::Absolute),
    OpCode::new(0x1E, "ASL", 3, 7, AddressingMode::Absolute_X),
    // Logical Shift Right (N,Z,C)
    OpCode::new(0x4a, "LSR", 1, 2, AddressingMode::NoneAddressing),
    OpCode::new(0x46, "LSR", 2, 5, AddressingMode::ZeroPage),
    OpCode::new(0x56, "LSR", 2, 6, AddressingMode::ZeroPage_X),
    OpCode::new(0x4e, "LSR", 3, 6, AddressingMode::Absolute),
    OpCode::new(0x5e, "LSR", 3, 7, AddressingMode::Absolute_X),
    // Rotate Left (N,Z,C)
    OpCode::new(0x2a, "ROL", 1, 2, AddressingMode::NoneAddressing),
    OpCode::new(0x26, "ROL", 2, 5, AddressingMode::ZeroPage),
    OpCode::new(0x36, "ROL", 2, 6, AddressingMode::ZeroPage_X),
    OpCode::new(0x2e, "ROL", 3, 6, AddressingMode::Absolute),
    OpCode::new(0x3e, "ROL", 3, 7, AddressingMode::Absolute_X),
    // Rotate Right (N,Z,C)
    OpCode::new(0x6a, "ROR", 1, 2, AddressingMode::NoneAddressing),
    OpCode::new(0x66, "ROR", 2, 5, AddressingMode::ZeroPage),
    OpCode::new(0x76, "ROR", 2, 6, AddressingMode::ZeroPage_X),
    OpCode::new(0x6e, "ROR", 3, 6, AddressingMode::Absolute),
    OpCode::new(0x7e, "ROR", 3, 7, AddressingMode::Absolute_X),

    /* --- Jumps & Calls --- */
    // Jump to another location
    OpCode::new(0x4c, "JMP", 3, 3, AddressingMode::Absolute),
    OpCode::new(0x6c, "JMP", 3, 5, AddressingMode::Indirect),
    // Jump to subroutine
    OpCode::new(0x20, "JSR", 3, 6, AddressingMode::Absolute),
    // Return from subroutine
    OpCode::new(0x60, "RTS", 1, 6, AddressingMode::NoneAddressing),

    /* --- Branches --- */
    // Branch if carry flag clear
    OpCode::new(0x90, "BCC", 2, 2, AddressingMode::NoneAddressing),
    // Branch if carry flag set
    OpCode::new(0xB0, "BCS", 2, 2, AddressingMode::NoneAddressing),
    // Branch if zero flag set
    OpCode::new(0xF0, "BEQ", 2, 2, AddressingMode::NoneAddressing),
    // Branch if negative flag set
    OpCode::new(0x30, "BMI", 2, 2, AddressingMode::NoneAddressing),
    // Branch if zero flag clear
    OpCode::new(0xD0, "BNE", 2, 2, AddressingMode::NoneAddressing),
    // Branch if negative flag clear
    OpCode::new(0x10, "BPL", 2, 2, AddressingMode::NoneAddressing),
    // Branch if overflow flag clear
    OpCode::new(0x50, "BVC", 2, 2, AddressingMode::NoneAddressing),
    // Branch if overflow flag set
    OpCode::new(0x70, "BVS", 2, 2, AddressingMode::NoneAddressing),

    /* --- Status Flag Changes --- */
    // Clear carry flag (C)
    OpCode::new(0x18, "CLC", 1, 2, AddressingMode::NoneAddressing),
    // Clear decimal mode flag (D)
    OpCode::new(0xD8, "CLD", 1, 2, AddressingMode::NoneAddressing),
    // Clear interrupt disable flag (I)
    OpCode::new(0x58, "CLI", 1, 2, AddressingMode::NoneAddressing),
    // Clear overflow flag (V)
    OpCode::new(0xB8, "CLV", 1, 2, AddressingMode::NoneAddressing),
    // set carry flag (C)
    OpCode::new(0x38, "SEC", 1, 2, AddressingMode::NoneAddressing),
    // set decimal mode flag (D)
    OpCode::new(0xF8, "SED", 1, 2, AddressingMode::NoneAddressing),
    // set interrupt disable flag (I)
    OpCode::new(0x78, "SEI", 1, 2, AddressingMode::NoneAddressing),

    /* --- System Functions --- */
    // Force an interrupt (B)
    OpCode::new(0x00, "BRK", 1, 7, AddressingMode::NoneAddressing),
    // No Operation
    OpCode::new(0xea, "NOP", 1, 2, AddressingMode::NoneAddressing),
    // Return from interrupt
    OpCode::new(0x40, "RTI", 1, 6, AddressingMode::NoneAddressing),
    ];

    pub static ref OPCODES_MAP: HashMap<u8, &'static OpCode> = {
        let mut map = HashMap::new();
        for cpuop in &*CPU_OPS_CODES {
            map.insert(cpuop.code, cpuop);
        }
        map
    };
}

pub struct OpCode {
    pub code: u8,
    pub mnemonic: &'static str,
    pub len: u8,
    pub cycles: u8,
    pub mode: AddressingMode,
}

impl OpCode {
    pub fn new(
        code: u8,
        mnemonic: &'static str,
        len: u8,
        cycles: u8,
        mode: AddressingMode,
    ) -> Self {
        Self {
            code,
            mnemonic,
            len,
            cycles,
            mode,
        }
    }
}
