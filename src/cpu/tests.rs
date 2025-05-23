use super::*;
use crate::rom::test::TestRom;
use std::vec;

fn init_cpu(instructions: Vec<u8>) -> CPU<'static> {
    let test_rom = TestRom::create_test_rom(instructions);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu
}

#[test]
fn test_0xa9_lda_immediate_load_data() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0x05, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();
    assert_eq!(cpu.register_a, 0x05);
    assert!(cpu.status.to_u8() & 0b000_0010 == 0b00);
    assert!(cpu.status.to_u8() & 0b1000_0000 == 0);
}

#[test]
fn test_0xa9_lda_zero_frag() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0x00, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();
    assert!(cpu.status.to_u8() & 0b000_0010 == 0b10);
}

#[test]
fn test_0xaa_tax_move_a_to_x() {
    let test_rom = TestRom::create_test_rom(vec![0xaa, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.register_a = 10;
    cpu.run();

    assert_eq!(cpu.register_x, 10)
}

#[test]
fn test_inx_overflow() {
    let test_rom = TestRom::create_test_rom(vec![0xe8, 0xe8, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.register_x = 0xff;
    cpu.run();

    assert_eq!(cpu.register_x, 1)
}

#[test]
fn test_5_ops_working_togather() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0xc0, 0xaa, 0xe8, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_x, 0xc1)
}

#[test]
fn test_lda_from_memory() {
    let test_rom = TestRom::create_test_rom(vec![0xa5, 0x10, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
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
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_a, 0x84);
    assert!(cpu.status.negative_flag);
    assert!(!cpu.status.overflow_flag);
    assert!(cpu.status.carry_flag);
}

#[test]
fn test_adc2() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0x50, 0x69, 0x50, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_a, 0xa0);
    assert!(cpu.status.negative_flag);
    assert!(cpu.status.overflow_flag);
    assert!(!cpu.status.carry_flag);
}

#[test]
fn test_adc3() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0xd0, 0x69, 0x90, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_a, 0x60);
    assert!(!cpu.status.negative_flag);
    assert!(cpu.status.overflow_flag);
    assert!(cpu.status.carry_flag);
}

#[test]
fn test_adc_ff() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0x7f, 0x69, 0x7f, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.status.carry_flag = true;
    cpu.run();

    assert_eq!(cpu.register_a, 0xff);
    assert!(cpu.status.negative_flag);
    assert!(cpu.status.overflow_flag);
    assert!(!cpu.status.carry_flag);
}

#[test]
fn test_adc_carry_in() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0x50, 0x69, 0x10, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.status.carry_flag = true;

    cpu.run();

    assert_eq!(cpu.register_a, 0x61);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.overflow_flag);
    assert!(!cpu.status.carry_flag);
}

#[test]
fn test_sbc() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0x50, 0xE9, 0xf0, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_a, 0x5f);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.overflow_flag);
    assert!(!cpu.status.carry_flag);
}

#[test]
fn test_sbc2() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0x50, 0xe9, 0xb0, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_a, 0x9f);
    assert!(cpu.status.negative_flag);
    assert!(cpu.status.overflow_flag);
    assert!(!cpu.status.carry_flag);
}

#[test]
fn test_sbc3() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0xd0, 0xe9, 0x70, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_a, 0x5f);
    assert!(!cpu.status.negative_flag);
    assert!(cpu.status.overflow_flag);
    assert!(cpu.status.carry_flag);
}

#[test]
fn test_and() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0b1010_1010, 0x29, 0b0101_0101, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_a, 0);

    let test_rom = TestRom::create_test_rom(vec![0xa9, 0b1010_1010, 0x29, 0b0101_1010, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_a, 0b0000_1010);
}

#[test]
fn test_asl_accumulator() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0x50, 0x0a, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_a, 0xa0);
    assert!(cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
    assert!(!cpu.status.carry_flag);

    let test_rom = TestRom::create_test_rom(vec![0xa9, 0xf0, 0x0a, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_a, 0xe0);
    assert!(cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
    assert!(cpu.status.carry_flag);
}

#[test]
fn test_asl() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0b1010_1010, 0x85, 0xc0, 0x06, 0xc0, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.mem_read(0xc0), 0b0101_0100);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
    assert!(cpu.status.carry_flag);
}

#[test]
fn test_bcc() {
    let test_rom = TestRom::create_test_rom(vec![0x90, 0x01, 0x00, 0xa9, 0x51, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_bcc2() {
    let test_rom =
        TestRom::create_test_rom(vec![0x90, 0x04, 0x00, 0xa9, 0x51, 0x00, 0x90, 0xFB, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_beq() {
    let test_rom = TestRom::create_test_rom(vec![0xf0, 0x01, 0x00, 0xa9, 0x51, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.status.zero_flag = true;
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_bmi() {
    let test_rom = TestRom::create_test_rom(vec![0x30, 0x01, 0x00, 0xa9, 0x51, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
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
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert!(!cpu.status.zero_flag);
    assert!(!cpu.status.overflow_flag);
    assert!(cpu.status.negative_flag);
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
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert!(cpu.status.zero_flag);
    assert!(!cpu.status.overflow_flag);
    assert!(!cpu.status.negative_flag);
}

#[test]
fn test_bne() {
    let test_rom = TestRom::create_test_rom(vec![0xd0, 0x01, 0x00, 0xa9, 0x51, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.status.zero_flag = false;
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_bpl() {
    let test_rom = TestRom::create_test_rom(vec![0x10, 0x01, 0x00, 0xa9, 0x51, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.status.negative_flag = false;
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_bvc() {
    let test_rom = TestRom::create_test_rom(vec![0x50, 0x01, 0x00, 0xa9, 0x51, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.status.overflow_flag = false;
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_bvs() {
    let test_rom = TestRom::create_test_rom(vec![0x70, 0x01, 0x00, 0xa9, 0x51, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.status.overflow_flag = true;
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_cmp() {
    let test_rom = TestRom::create_test_rom(vec![0xc9, 0x51, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.register_a = 0x51;
    cpu.run();

    assert!(cpu.status.carry_flag);
    assert!(cpu.status.zero_flag);
    assert!(!cpu.status.negative_flag);
}

#[test]
fn test_cpx() {
    let test_rom = TestRom::create_test_rom(vec![0xe0, 0x51, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.register_x = 0x51;
    cpu.run();

    assert!(cpu.status.carry_flag);
    assert!(cpu.status.zero_flag);
    assert!(!cpu.status.negative_flag);
}

#[test]
fn test_cpy() {
    let test_rom = TestRom::create_test_rom(vec![0xc0, 0x51, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.register_y = 0x51;
    cpu.run();

    assert!(cpu.status.carry_flag);
    assert!(cpu.status.zero_flag);
    assert!(!cpu.status.negative_flag);
}

#[test]
fn test_dec() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0x51, 0x85, 0xc0, 0xc6, 0xc0, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.mem_read(0xc0), 0x50);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_dex() {
    let test_rom = TestRom::create_test_rom(vec![0xca, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.register_x = 0x51;
    cpu.run();

    assert_eq!(cpu.register_x, 0x50);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_dey() {
    let test_rom = TestRom::create_test_rom(vec![0x88, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.register_y = 0x51;
    cpu.run();

    assert_eq!(cpu.register_y, 0x50);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_eor() {
    let test_rom = TestRom::create_test_rom(vec![0x49, 0x51, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.register_a = 0x51;
    cpu.run();

    assert_eq!(cpu.register_a, 0x00);
    assert!(!cpu.status.negative_flag);
    assert!(cpu.status.zero_flag);
}

#[test]
fn test_inc() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0x51, 0x85, 0xc0, 0xe6, 0xc0, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.mem_read(0xc0), 0x52);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_iny() {
    let test_rom = TestRom::create_test_rom(vec![0xc8, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.register_y = 0x51;
    cpu.run();

    assert_eq!(cpu.register_y, 0x52);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_jmp() {
    let test_rom = TestRom::create_test_rom(vec![
        0xa9, 0x01, 0x85, 0xf0, 0xa9, 0xcc, 0x85, 0xf1, 0x6c, 0xf0, 0x00,
    ]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.program_counter, 0xcc02);
}

#[test]
fn test_jsr() {
    let test_rom = TestRom::create_test_rom(vec![0x20, 0x03, 0x80, 0xa9, 0x51, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
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
    let test_rom = TestRom::create_test_rom(vec![0x20, 0x04, 0x80, 0x00, 0xa9, 0x51, 0x60, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.stack_pointer, STACK_RESET);
    assert_eq!(cpu.program_counter, 0x8004);
}

#[test]
fn test_ldx() {
    let test_rom = TestRom::create_test_rom(vec![0xa2, 0x51, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_x, 0x51);
}

#[test]
fn test_ldy() {
    let test_rom = TestRom::create_test_rom(vec![0xa0, 0x51, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_y, 0x51);
}

#[test]
fn test_lsr_accumulator() {
    let test_rom = TestRom::create_test_rom(vec![0x4a, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.register_a = 0b0101_0101;
    cpu.run();

    assert_eq!(cpu.register_a, 0b0010_1010);
    assert!(cpu.status.carry_flag);
}

#[test]
fn test_ora() {
    let test_rom = TestRom::create_test_rom(vec![0x09, 0b0101_0101, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.register_a = 0b1010_1010;
    cpu.run();

    assert_eq!(cpu.register_a, 0b1111_1111);
}

#[test]
fn test_pha() {
    let test_rom = TestRom::create_test_rom(vec![0x48, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.register_a = 0x51;
    cpu.run();

    assert_eq!(cpu.stack_pop(), 0x51);
}

#[test]
fn test_php() {
    let test_rom = TestRom::create_test_rom(vec![0x08, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.status.carry_flag = true;
    cpu.run();

    assert_eq!(cpu.stack_pop(), 0b0011_0101);
}

#[test]
fn test_pla() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0x51, 0x48, 0xa9, 0x50, 0x68, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_plp() {
    let test_rom = TestRom::create_test_rom(vec![0x08, 0x38, 0x28, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert!(!cpu.status.carry_flag);
}

#[test]
fn test_rol_accumulator() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0b1010_1010, 0x2a, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.status.carry_flag = true;
    cpu.run();

    assert_eq!(cpu.register_a, 0b0101_0101);
    assert!(cpu.status.carry_flag);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_rol() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0b1010_1010, 0x85, 0xc0, 0x26, 0xc0, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.status.carry_flag = true;
    cpu.run();

    assert_eq!(cpu.mem_read(0xc0), 0b0101_0101);
    assert!(cpu.status.carry_flag);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_ror_accumulator() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0b1010_1010, 0x6a, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.status.carry_flag = true;
    cpu.run();

    assert_eq!(cpu.register_a, 0b1101_0101);
    assert!(!cpu.status.carry_flag);
    assert!(cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_ror() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0b1010_1010, 0x85, 0xc0, 0x66, 0xc0, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.status.carry_flag = true;
    cpu.run();

    assert_eq!(cpu.mem_read(0xc0), 0b1101_0101);
    assert!(!cpu.status.carry_flag);
    assert!(cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_stx() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0x51, 0xaa, 0x86, 0xc0, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.mem_read(0xc0), 0x51);
}

#[test]
fn test_sty() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0x51, 0xa8, 0x84, 0xc0, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.mem_read(0xc0), 0x51);
}

#[test]
fn test_tay() {
    let test_rom = TestRom::create_test_rom(vec![0xa9, 0x51, 0xa8, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_y, 0x51);
}

#[test]
fn test_tsx() {
    let test_rom = TestRom::create_test_rom(vec![0xba, 0x00]);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.reset();
    cpu.run();

    assert_eq!(cpu.register_x, 0xfd);
}

// TODO: write tests for Unofficial Opcodes
