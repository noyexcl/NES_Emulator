use std::collections::HashMap;

use crate::{
    cpu::{AddressingMode, CPU},
    opcodes,
};

/// Provide a way to view the value at a specific address without causing side effects for logging.
pub trait Inspector {
    /// Get current state of the address. \
    /// There is no side effect such as status reset or clearing flags when reading certain registers. \
    /// This fucntion is intended to be used to log the value of the address.
    ///
    /// If the address points to write-only region or meaningless value to read, it will return 0xFF.
    fn inspect(&self, addr: u16) -> u8;

    fn inspect_u16(&self, addr: u16) -> u16;
}

const UNOFFICIAL_OPCODES: [u8; 80] = [
    0x1A, 0x3A, 0x5A, 0x7A, 0xDA, 0xFA, 0x04, 0x44, 0x64, 0x0c, 0x1c, 0x3C, 0x5C, 0x7C, 0xDC, 0xFC,
    0x14, 0x34, 0x54, 0x74, 0xD4, 0xF4, 0x80, 0x82, 0x89, 0xC2, 0xE2, 0xA7, 0xB7, 0xAF, 0xBF, 0xA3,
    0xB3, 0x87, 0x97, 0x8F, 0x83, 0xEB, 0xC7, 0xD7, 0xCF, 0xDF, 0xDB, 0xC3, 0xD3, 0xE7, 0xF7, 0xEF,
    0xFF, 0xFB, 0xE3, 0xF3, 0x07, 0x17, 0x0F, 0x1F, 0x1B, 0x03, 0x13, 0x27, 0x37, 0x2F, 0x3F, 0x3B,
    0x23, 0x33, 0x47, 0x57, 0x4F, 0x5F, 0x5B, 0x43, 0x53, 0x67, 0x77, 0x6F, 0x7F, 0x7B, 0x63, 0x73,
];

pub fn trace(cpu: &CPU) -> String {
    let mut result = String::new();
    let opcode_table: &HashMap<u8, &'static opcodes::OpCode> = &opcodes::OPCODES_MAP;
    let code = cpu.bus.inspect(cpu.program_counter);
    let opcode = opcode_table
        .get(&code)
        .unwrap_or_else(|| panic!("OpCode {:02X} is not recognized", code));

    // program counter & opcode
    result.push_str(&format!("{:04X}  ", cpu.program_counter));
    result.push_str(&format!("{:02X} ", opcode.code));

    // following opcodes
    if opcode.len == 1 {
        result.push_str("      ");
    } else if opcode.len == 2 {
        result.push_str(&format!(
            "{:02X}    ",
            cpu.bus.inspect(cpu.program_counter + 1)
        ));
    } else if opcode.len == 3 {
        result.push_str(&format!(
            "{:02X} {:02X} ",
            cpu.bus.inspect(cpu.program_counter + 1),
            cpu.bus.inspect(cpu.program_counter + 2)
        ));
    }

    let unofficial_op = UNOFFICIAL_OPCODES.iter().any(|&x| x == opcode.code);

    result.push_str(&format!(
        "{}{} ",
        if unofficial_op { "*" } else { " " },
        opcode.mnemonic
    ));

    // mnemonic & formatted operand
    match opcode.mode {
        AddressingMode::Immediate => {
            let value = cpu.bus.inspect(cpu.program_counter + 1);

            // mnemonic & value with format
            result.push_str(&format!("{:28}", format!("#${:02X}", value)));
        }
        AddressingMode::ZeroPage => {
            let addr = get_operand_address(cpu, &opcode.mode);
            let value = cpu.bus.inspect(addr);

            // mnemonic & addr with format
            result.push_str(&format!("{:28}", format!("${:02X} = {:02X}", addr, value)));
        }
        AddressingMode::ZeroPage_X => {
            let base = cpu.bus.inspect(cpu.program_counter + 1);
            let addr = get_operand_address(cpu, &opcode.mode);
            let value = cpu.bus.inspect(addr);

            result.push_str(&format!(
                "{:28}",
                format!("${:02X},X @ {:02X} = {:02X}", base, addr as u8, value)
            ));
        }

        AddressingMode::ZeroPage_Y => {
            let base = cpu.bus.inspect(cpu.program_counter + 1);
            let addr = get_operand_address(cpu, &opcode.mode);
            let value = cpu.bus.inspect(addr);

            result.push_str(&format!(
                "{:28}",
                format!("${:02X},Y @ {:02X} = {:02X}", base, addr as u8, value)
            ));
        }
        AddressingMode::Absolute => {
            let addr = get_operand_address(cpu, &opcode.mode);
            let value = cpu.bus.inspect(addr);

            match opcode.code {
                // JMP系の命令の場合、値は表示しない
                0x4c | 0x20 => {
                    result.push_str(&format!("{:28}", format!("${:04X}", addr)));
                }
                _ => {
                    result.push_str(&format!("{:28}", format!("${:04X} = {:02X}", addr, value)));
                }
            }
        }
        AddressingMode::Absolute_X => {
            let lo = cpu.bus.inspect(cpu.program_counter + 1) as u16;
            let hi = cpu.bus.inspect(cpu.program_counter + 2) as u16;
            let addr = (hi << 8) | lo;

            let indexed_addr = addr.wrapping_add(cpu.register_x as u16);
            let value = cpu.bus.inspect(indexed_addr);

            result.push_str(&format!(
                "{:28}",
                format!("${:04X},X @ {:04X} = {:02X}", addr, indexed_addr, value)
            ));
        }
        AddressingMode::Absolute_Y => {
            let lo = cpu.bus.inspect(cpu.program_counter + 1) as u16;
            let hi = cpu.bus.inspect(cpu.program_counter + 2) as u16;
            let addr = (hi << 8) | lo;

            let indexed_addr = addr.wrapping_add(cpu.register_y as u16);
            let value = cpu.bus.inspect(indexed_addr);

            result.push_str(&format!(
                "{:28}",
                format!("${:04X},Y @ {:04X} = {:02X}", addr, indexed_addr, value)
            ));
        }
        AddressingMode::Indirect => {
            let lo = cpu.bus.inspect(cpu.program_counter + 1) as u16;
            let hi = cpu.bus.inspect(cpu.program_counter + 2) as u16;
            let addr = (hi << 8) | lo;

            let jmp_addr = get_operand_address(cpu, &opcode.mode);

            result.push_str(&format!(
                "{:28}",
                format!("(${:04X}) = {:04X}", addr, jmp_addr)
            ));
        }
        AddressingMode::Indirect_X => {
            let base = cpu.bus.inspect(cpu.program_counter + 1);
            let addr = get_operand_address(cpu, &opcode.mode);
            let value = cpu.bus.inspect(addr);

            result.push_str(&format!(
                "{:28}",
                format!(
                    "(${:02X},X) @ {:02X} = {:04X} = {:02X}",
                    base,
                    base.wrapping_add(cpu.register_x),
                    addr,
                    value
                )
            ));
        }
        AddressingMode::Indirect_Y => {
            let base = cpu.bus.inspect(cpu.program_counter + 1);
            let addr = get_operand_address(cpu, &opcode.mode);
            let addr_before_indexed = addr.wrapping_sub(cpu.register_y as u16);
            let value = cpu.bus.inspect(addr);

            result.push_str(&format!(
                "{:28}",
                format!(
                    "(${:02X}),Y = {:04X} @ {:04X} = {:02X}",
                    base, addr_before_indexed, addr, value
                )
            ));
        }
        AddressingMode::NoneAddressing => match opcode.code {
            // ブランチ系のRelativeアドレッシングモードでは、ジャンプ先のアドレスを計算して表示する
            0x90 | 0xB0 | 0xF0 | 0x30 | 0xD0 | 0x10 | 0x50 | 0x70 => {
                let offset = cpu.bus.inspect(cpu.program_counter + 1) as i8 as i16;
                let jmp_addr = ((cpu.program_counter + 2) as i16).wrapping_add(offset) as u16;

                result.push_str(&format!("{:28}", format!("${:04X}", jmp_addr)));
            }
            // Accumulatorアドレッシングモードの場合、Aと表示する
            0x4A | 0x0A | 0x6A | 0x2A => {
                result.push_str(&format!("{:28}", "A"));
            }
            _ => {
                result.push_str(&format!("{:28}", ""));
            }
        },
    }

    result.push_str(&format!(
        "A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} ",
        cpu.register_a,
        cpu.register_x,
        cpu.register_y,
        cpu.status.to_u8(),
        cpu.stack_pointer
    ));

    result.push_str(&format!(
        "PPU:{:3},{:3} CYC:{}",
        (cpu.bus.cycles * 3) / 341,
        (cpu.bus.cycles * 3) % 341,
        cpu.bus.cycles,
    ));

    result
}

/// Trace in Messen2 format
pub fn trace2(cpu: &CPU) -> String {
    let mut result = String::new();
    let opcode_table: &HashMap<u8, &'static opcodes::OpCode> = &opcodes::OPCODES_MAP;
    let code = cpu.bus.inspect(cpu.program_counter);
    let opcode = opcode_table
        .get(&code)
        .unwrap_or_else(|| panic!("OpCode {:02X} is not recognized", code));

    // program counter
    result.push_str(&format!("{:04X}  ", cpu.program_counter));

    result.push_str(&format!("{} ", opcode.mnemonic));

    // mnemonic & formatted operand
    match opcode.mode {
        AddressingMode::Immediate => {
            let value = cpu.bus.inspect(cpu.program_counter + 1);

            // mnemonic & value with format
            result.push_str(&format!("{:29}", format!("#${:02X}", value)));
        }
        AddressingMode::ZeroPage => {
            let addr = get_operand_address(cpu, &opcode.mode);
            let value = cpu.bus.inspect(addr);

            // mnemonic & addr with format
            result.push_str(&format!("{:29}", format!("${:02X} = ${:02X}", addr, value)));
        }
        AddressingMode::ZeroPage_X => {
            let base = cpu.bus.inspect(cpu.program_counter + 1);
            let addr = get_operand_address(cpu, &opcode.mode);
            let value = cpu.bus.inspect(addr);

            result.push_str(&format!(
                "{:29}",
                format!("${:02X},X [${:04X}] = ${:02X}", base, addr as u8, value)
            ));
        }

        AddressingMode::ZeroPage_Y => {
            let base = cpu.bus.inspect(cpu.program_counter + 1);
            let addr = get_operand_address(cpu, &opcode.mode);
            let value = cpu.bus.inspect(addr);

            result.push_str(&format!(
                "{:29}",
                format!("${:02X},Y [${:04X}] = ${:02X}", base, addr as u8, value)
            ));
        }
        AddressingMode::Absolute => {
            let addr = get_operand_address(cpu, &opcode.mode);
            let value = cpu.bus.inspect(addr);

            match opcode.code {
                // JMP系の命令の場合、値は表示しない
                0x4c | 0x20 => {
                    result.push_str(&format!("{:29}", format!("${:04X}", addr)));
                }
                _ => {
                    result.push_str(&format!("{:29}", format!("${:04X} = ${:02X}", addr, value)));
                }
            }
        }
        AddressingMode::Absolute_X => {
            let lo = cpu.bus.inspect(cpu.program_counter + 1) as u16;
            let hi = cpu.bus.inspect(cpu.program_counter + 2) as u16;
            let addr = (hi << 8) | lo;

            let indexed_addr = addr.wrapping_add(cpu.register_x as u16);
            let value = cpu.bus.inspect(indexed_addr);

            result.push_str(&format!(
                "{:29}",
                format!("${:04X},X [${:04X}] = ${:02X}", addr, indexed_addr, value)
            ));
        }
        AddressingMode::Absolute_Y => {
            let lo = cpu.bus.inspect(cpu.program_counter + 1) as u16;
            let hi = cpu.bus.inspect(cpu.program_counter + 2) as u16;
            let addr = (hi << 8) | lo;

            let indexed_addr = addr.wrapping_add(cpu.register_y as u16);
            let value = cpu.bus.inspect(indexed_addr);

            result.push_str(&format!(
                "{:29}",
                format!("${:04X},Y [${:04X}] = ${:02X}", addr, indexed_addr, value)
            ));
        }
        AddressingMode::Indirect => {
            let lo = cpu.bus.inspect(cpu.program_counter + 1) as u16;
            let hi = cpu.bus.inspect(cpu.program_counter + 2) as u16;
            let addr = (hi << 8) | lo;

            let jmp_addr = get_operand_address(cpu, &opcode.mode);
            let value = cpu.bus.inspect(jmp_addr);

            result.push_str(&format!(
                "{:29}",
                format!("(${:04X}) [${:04X}] = ${:02X}", addr, jmp_addr, value)
            ));
        }
        AddressingMode::Indirect_X => {
            let base = cpu.bus.inspect(cpu.program_counter + 1);
            let addr = get_operand_address(cpu, &opcode.mode);
            let value = cpu.bus.inspect(addr);

            result.push_str(&format!(
                "{:29}",
                format!(
                    "(${:02X},X) [${:04X}] = ${:02X}",
                    base,
                    addr,
                    value
                )
            ));
        }
        AddressingMode::Indirect_Y => {
            let base = cpu.bus.inspect(cpu.program_counter + 1);
            let addr = get_operand_address(cpu, &opcode.mode);
            let addr_before_indexed = addr.wrapping_sub(cpu.register_y as u16);
            let value = cpu.bus.inspect(addr);

            result.push_str(&format!(
                "{:29}",
                format!(
                    "(${:02X}),Y [${:04X}] = ${:02X}",
                    base, addr, value
                )
            ));
        }
        AddressingMode::NoneAddressing => match opcode.code {
            // ブランチ系のRelativeアドレッシングモードでは、ジャンプ先のアドレスを計算して表示する
            0x90 | 0xB0 | 0xF0 | 0x30 | 0xD0 | 0x10 | 0x50 | 0x70 => {
                let offset = cpu.bus.inspect(cpu.program_counter + 1) as i8 as i16;
                let jmp_addr = ((cpu.program_counter + 2) as i16).wrapping_add(offset) as u16;

                result.push_str(&format!("{:29}", format!("${:04X}", jmp_addr)));
            }
            // Accumulatorアドレッシングモードの場合、Aと表示する
            0x4A | 0x0A | 0x6A | 0x2A => {
                result.push_str(&format!("{:29}", "A"));
            }
            _ => {
                result.push_str(&format!("{:29}", ""));
            }
        },
    }

    result.push_str(&format!(
        "A:{:02X} X:{:02X} Y:{:02X} S:{:02X} P:{:02X} ",
        cpu.register_a,
        cpu.register_x,
        cpu.register_y,
        cpu.stack_pointer,
        cpu.status.to_u8() & 0b1101_1111
    ));

    result.push_str(&format!("BC:{:02X} ", opcode.code));

    // following opcodes
    if opcode.len == 2 {
        result.push_str(&format!("{:02X}", cpu.bus.inspect(cpu.program_counter + 1)));
    } else if opcode.len == 3 {
        result.push_str(&format!(
            "{:02X} {:02X}",
            cpu.bus.inspect(cpu.program_counter + 1),
            cpu.bus.inspect(cpu.program_counter + 2)
        ));
    }

    result
}

fn get_operand_address(cpu: &CPU, mode: &AddressingMode) -> u16 {
    let counter = cpu.program_counter + 1;

    match mode {
        AddressingMode::Immediate => counter,
        AddressingMode::ZeroPage => cpu.bus.inspect(counter) as u16,
        AddressingMode::Absolute => cpu.bus.inspect_u16(counter),

        AddressingMode::ZeroPage_X => {
            let pos = cpu.bus.inspect(counter);
            pos.wrapping_add(cpu.register_x) as u16
        }
        AddressingMode::ZeroPage_Y => {
            let pos = cpu.bus.inspect(counter);
            pos.wrapping_add(cpu.register_y) as u16
        }
        AddressingMode::Absolute_X => {
            let pos = cpu.bus.inspect(counter);
            pos.wrapping_add(cpu.register_x) as u16
        }
        AddressingMode::Absolute_Y => {
            let base = cpu.bus.inspect_u16(counter);
            base.wrapping_add(cpu.register_y as u16)
        }
        AddressingMode::Indirect => {
            // 6502にはページをまたぐIndirectにバグが存在している
            // 例えば、JMP ($30FF) という命令の場合、
            // 本来は、$30FFにある値(下位バイト)と$3100(上位バイト)にある値を参照しなければならないが
            // $30FF(下位バイト)と$3000(上位バイト)の値を参照してしまう
            // ここではそれを再現している
            let addr = cpu.bus.inspect_u16(counter);

            // 対象のアドレスがFFで終わる場合、つまりページをまたぐ場合はバグを再現
            if addr & 0x00FF == 0x00FF {
                let lo = cpu.bus.inspect(addr);
                let hi = cpu.bus.inspect(addr & 0xFF00);
                (hi as u16) << 8 | (lo as u16)
            } else {
                cpu.bus.inspect_u16(addr)
            }
        }
        AddressingMode::Indirect_X => {
            let base = cpu.bus.inspect(counter);

            let ptr: u8 = base.wrapping_add(cpu.register_x);
            let lo = cpu.bus.inspect(ptr as u16);
            let hi = cpu.bus.inspect(ptr.wrapping_add(1) as u16);
            (hi as u16) << 8 | (lo as u16)
        }
        AddressingMode::Indirect_Y => {
            let base: u8 = cpu.bus.inspect(counter);
            let lo = cpu.bus.inspect(base as u16);
            let hi = cpu.bus.inspect(base.wrapping_add(1) as u16);
            let deref_base = (hi as u16) << 8 | (lo as u16);
            deref_base.wrapping_add(cpu.register_y as u16)
        }
        AddressingMode::NoneAddressing => {
            panic!("mode {:?} is not supported", mode);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::bus::Bus;
    use crate::mem::Mem;
    use crate::rom::test::TestRom;

    #[test]
    fn test_format_trace() {
        let mut bus = Bus::new(TestRom::create_test_rom(vec![]), |_, _, _| {});
        bus.mem_write(100, 0xa2);
        bus.mem_write(101, 0x01);
        bus.mem_write(102, 0xca);
        bus.mem_write(103, 0x88);
        bus.mem_write(104, 0x00);

        let mut cpu = CPU::new(bus);
        cpu.reset();
        cpu.program_counter = 0x64;
        cpu.register_a = 1;
        cpu.register_x = 2;
        cpu.register_y = 3;
        let mut result: Vec<String> = vec![];
        cpu.run_with_callback(|cpu| {
            result.push(trace(cpu));
        });
        assert_eq!(
            "0064  A2 01     LDX #$01                        A:01 X:02 Y:03 P:24 SP:FD PPU:  0, 21 CYC:7",
            result[0]
        );
        assert_eq!(
            "0066  CA        DEX                             A:01 X:01 Y:03 P:24 SP:FD PPU:  0, 27 CYC:9",
            result[1]
        );
        assert_eq!(
            "0067  88        DEY                             A:01 X:00 Y:03 P:26 SP:FD PPU:  0, 33 CYC:11",
            result[2]
        );
    }

    #[test]
    fn test_format_mem_access() {
        let mut bus = Bus::new(TestRom::create_test_rom(vec![]), |_, _, _| {});
        // ORA ($33), Y
        bus.mem_write(100, 0x11);
        bus.mem_write(101, 0x33);

        //data
        bus.mem_write(0x33, 00);
        bus.mem_write(0x34, 4);

        //target cell
        bus.mem_write(0x400, 0xAA);

        let mut cpu = CPU::new(bus);
        cpu.reset();
        cpu.program_counter = 0x64;
        cpu.register_y = 0;
        let mut result: Vec<String> = vec![];
        cpu.run_with_callback(|cpu| {
            result.push(trace(cpu));
        });
        assert_eq!(
            "0064  11 33     ORA ($33),Y = 0400 @ 0400 = AA  A:00 X:00 Y:00 P:24 SP:FD PPU:  0, 21 CYC:7",
            result[0]
        );
    }

    #[test]
    fn test_zerox_format() {
        let mut bus = Bus::new(TestRom::create_test_rom(vec![]), |_, _, _| {});
        // ORA ($33), Y
        bus.mem_write(100, 0xb5);
        bus.mem_write(101, 0x33);

        //data
        bus.mem_write(0x33, 0xFF);
        bus.mem_write(0x34, 0xAA);

        let mut cpu = CPU::new(bus);
        cpu.reset();
        cpu.program_counter = 0x64;
        cpu.register_x = 1;
        let mut result: Vec<String> = vec![];
        cpu.run_with_callback(|cpu| {
            result.push(trace(cpu));
        });

        assert_eq!(
            "0064  B5 33     LDA $33,X @ 34 = AA             A:00 X:01 Y:00 P:24 SP:FD PPU:  0, 21 CYC:7",
            result[0]
        );
    }

    #[test]
    fn test_zeroy_format() {
        let mut bus = Bus::new(TestRom::create_test_rom(vec![]), |_, _, _| {});
        bus.mem_write(100, 0xb6);
        bus.mem_write(101, 0x33);

        //data
        bus.mem_write(0x33, 0xFF);
        bus.mem_write(0x34, 0xAA);

        let mut cpu = CPU::new(bus);
        cpu.reset();
        cpu.program_counter = 0x64;
        cpu.register_y = 1;
        let mut result: Vec<String> = vec![];
        cpu.run_with_callback(|cpu| {
            result.push(trace(cpu));
        });

        assert_eq!(
            "0064  B6 33     LDX $33,Y @ 34 = AA             A:00 X:00 Y:01 P:24 SP:FD PPU:  0, 21 CYC:7",
            result[0]
        );
    }
}
