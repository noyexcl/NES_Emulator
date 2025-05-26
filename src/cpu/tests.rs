use super::*;
use crate::rom::test::TestRom;
use std::vec;

fn init_cpu(instructions: Vec<u8>) -> CPU<'static> {
    let test_rom = TestRom::create_test_rom(instructions);
    let mut cpu = CPU::new(Bus::new(test_rom, |_, _, _| {}));
    cpu.exit_on_brk = true;
    cpu.reset();
    cpu
}

#[test]
fn test_0xa9_lda_immediate_load_data() {
    let mut cpu = init_cpu(vec![0xa9, 0x05, 0x00]);
    cpu.run();
    assert_eq!(cpu.register_a, 0x05);
    assert!(cpu.status.to_u8() & 0b000_0010 == 0b00);
    assert!(cpu.status.to_u8() & 0b1000_0000 == 0);
}

#[test]
fn test_0xa9_lda_zero_frag() {
    let mut cpu = init_cpu(vec![0xa9, 0x00, 0x00]);
    cpu.run();
    assert!(cpu.status.to_u8() & 0b000_0010 == 0b10);
}

#[test]
fn test_0xaa_tax_move_a_to_x() {
    let mut cpu = init_cpu(vec![0xaa, 0x00]);
    cpu.register_a = 10;
    cpu.run();

    assert_eq!(cpu.register_x, 10)
}

#[test]
fn test_inx_overflow() {
    let mut cpu = init_cpu(vec![0xe8, 0xe8, 0x00]);
    cpu.register_x = 0xff;
    cpu.run();

    assert_eq!(cpu.register_x, 1)
}

#[test]
fn test_5_ops_working_togather() {
    let mut cpu = init_cpu(vec![0xa9, 0xc0, 0xaa, 0xe8, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_x, 0xc1)
}

#[test]
fn test_lda_from_memory() {
    let mut cpu = init_cpu(vec![0xa5, 0x10, 0x00]);
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
    let mut cpu = init_cpu(vec![0xa9, 0xc0, 0x69, 0xc4, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_a, 0x84);
    assert!(cpu.status.negative_flag);
    assert!(!cpu.status.overflow_flag);
    assert!(cpu.status.carry_flag);
}

#[test]
fn test_adc2() {
    let mut cpu = init_cpu(vec![0xa9, 0x50, 0x69, 0x50, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_a, 0xa0);
    assert!(cpu.status.negative_flag);
    assert!(cpu.status.overflow_flag);
    assert!(!cpu.status.carry_flag);
}

#[test]
fn test_adc3() {
    let mut cpu = init_cpu(vec![0xa9, 0xd0, 0x69, 0x90, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_a, 0x60);
    assert!(!cpu.status.negative_flag);
    assert!(cpu.status.overflow_flag);
    assert!(cpu.status.carry_flag);
}

#[test]
fn test_adc_ff() {
    let mut cpu = init_cpu(vec![0xa9, 0x7f, 0x69, 0x7f, 0x00]);
    cpu.status.carry_flag = true;
    cpu.run();

    assert_eq!(cpu.register_a, 0xff);
    assert!(cpu.status.negative_flag);
    assert!(cpu.status.overflow_flag);
    assert!(!cpu.status.carry_flag);
}

#[test]
fn test_adc_carry_in() {
    let mut cpu = init_cpu(vec![0xa9, 0x50, 0x69, 0x10, 0x00]);
    cpu.status.carry_flag = true;

    cpu.run();

    assert_eq!(cpu.register_a, 0x61);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.overflow_flag);
    assert!(!cpu.status.carry_flag);
}

#[test]
fn test_sbc() {
    let mut cpu = init_cpu(vec![0xa9, 0x50, 0xE9, 0xf0, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_a, 0x5f);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.overflow_flag);
    assert!(!cpu.status.carry_flag);
}

#[test]
fn test_sbc2() {
    let mut cpu = init_cpu(vec![0xa9, 0x50, 0xe9, 0xb0, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_a, 0x9f);
    assert!(cpu.status.negative_flag);
    assert!(cpu.status.overflow_flag);
    assert!(!cpu.status.carry_flag);
}

#[test]
fn test_sbc3() {
    let mut cpu = init_cpu(vec![0xa9, 0xd0, 0xe9, 0x70, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_a, 0x5f);
    assert!(!cpu.status.negative_flag);
    assert!(cpu.status.overflow_flag);
    assert!(cpu.status.carry_flag);
}

#[test]
fn test_and() {
    let mut cpu = init_cpu(vec![0xa9, 0b1010_1010, 0x29, 0b0101_0101, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_a, 0);

    let mut cpu = init_cpu(vec![0xa9, 0b1010_1010, 0x29, 0b0101_1010, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_a, 0b0000_1010);
}

#[test]
fn test_asl_accumulator() {
    let mut cpu = init_cpu(vec![0xa9, 0x50, 0x0a, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_a, 0xa0);
    assert!(cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
    assert!(!cpu.status.carry_flag);

    let mut cpu = init_cpu(vec![0xa9, 0xf0, 0x0a, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_a, 0xe0);
    assert!(cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
    assert!(cpu.status.carry_flag);
}

#[test]
fn test_asl() {
    let mut cpu = init_cpu(vec![0xa9, 0b1010_1010, 0x85, 0xc0, 0x06, 0xc0, 0x00]);
    cpu.run();

    assert_eq!(cpu.mem_read(0xc0), 0b0101_0100);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
    assert!(cpu.status.carry_flag);
}

#[test]
fn test_bcc() {
    let mut cpu = init_cpu(vec![0x90, 0x01, 0x00, 0xa9, 0x51, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_bcc2() {
    let mut cpu = init_cpu(vec![0x90, 0x04, 0x00, 0xa9, 0x51, 0x00, 0x90, 0xFB, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_beq() {
    let mut cpu = init_cpu(vec![0xf0, 0x01, 0x00, 0xa9, 0x51, 0x00]);
    cpu.status.zero_flag = true;
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_bmi() {
    let mut cpu = init_cpu(vec![0x30, 0x01, 0x00, 0xa9, 0x51, 0x00]);
    cpu.status.negative_flag = true;
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_bit() {
    let mut cpu = init_cpu(vec![
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
    cpu.run();

    assert!(!cpu.status.zero_flag);
    assert!(!cpu.status.overflow_flag);
    assert!(cpu.status.negative_flag);
}

#[test]
fn test_bit2() {
    let mut cpu = init_cpu(vec![
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
    cpu.run();

    assert!(cpu.status.zero_flag);
    assert!(!cpu.status.overflow_flag);
    assert!(!cpu.status.negative_flag);
}

#[test]
fn test_bne() {
    let mut cpu = init_cpu(vec![0xd0, 0x01, 0x00, 0xa9, 0x51, 0x00]);
    cpu.status.zero_flag = false;
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_bpl() {
    let mut cpu = init_cpu(vec![0x10, 0x01, 0x00, 0xa9, 0x51, 0x00]);
    cpu.status.negative_flag = false;
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_bvc() {
    let mut cpu = init_cpu(vec![0x50, 0x01, 0x00, 0xa9, 0x51, 0x00]);
    cpu.status.overflow_flag = false;
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_bvs() {
    let mut cpu = init_cpu(vec![0x70, 0x01, 0x00, 0xa9, 0x51, 0x00]);
    cpu.status.overflow_flag = true;
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_cmp() {
    let mut cpu = init_cpu(vec![0xc9, 0x51, 0x00]);
    cpu.register_a = 0x51;
    cpu.run();

    assert!(cpu.status.carry_flag);
    assert!(cpu.status.zero_flag);
    assert!(!cpu.status.negative_flag);
}

#[test]
fn test_cpx() {
    let mut cpu = init_cpu(vec![0xe0, 0x51, 0x00]);
    cpu.register_x = 0x51;
    cpu.run();

    assert!(cpu.status.carry_flag);
    assert!(cpu.status.zero_flag);
    assert!(!cpu.status.negative_flag);
}

#[test]
fn test_cpy() {
    let mut cpu = init_cpu(vec![0xc0, 0x51, 0x00]);
    cpu.register_y = 0x51;
    cpu.run();

    assert!(cpu.status.carry_flag);
    assert!(cpu.status.zero_flag);
    assert!(!cpu.status.negative_flag);
}

#[test]
fn test_dec() {
    let mut cpu = init_cpu(vec![0xa9, 0x51, 0x85, 0xc0, 0xc6, 0xc0, 0x00]);
    cpu.run();

    assert_eq!(cpu.mem_read(0xc0), 0x50);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_dex() {
    let mut cpu = init_cpu(vec![0xca, 0x00]);
    cpu.register_x = 0x51;
    cpu.run();

    assert_eq!(cpu.register_x, 0x50);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_dey() {
    let mut cpu = init_cpu(vec![0x88, 0x00]);
    cpu.register_y = 0x51;
    cpu.run();

    assert_eq!(cpu.register_y, 0x50);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_eor() {
    let mut cpu = init_cpu(vec![0x49, 0x51, 0x00]);
    cpu.register_a = 0x51;
    cpu.run();

    assert_eq!(cpu.register_a, 0x00);
    assert!(!cpu.status.negative_flag);
    assert!(cpu.status.zero_flag);
}

#[test]
fn test_inc() {
    let mut cpu = init_cpu(vec![0xa9, 0x51, 0x85, 0xc0, 0xe6, 0xc0, 0x00]);
    cpu.run();

    assert_eq!(cpu.mem_read(0xc0), 0x52);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_iny() {
    let mut cpu = init_cpu(vec![0xc8, 0x00]);
    cpu.register_y = 0x51;
    cpu.run();

    assert_eq!(cpu.register_y, 0x52);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_jmp() {
    let mut cpu = init_cpu(vec![
        0xa9, 0x01, 0x85, 0xf0, 0xa9, 0xcc, 0x85, 0xf1, 0x6c, 0xf0, 0x00,
    ]);
    cpu.run();

    assert_eq!(cpu.program_counter, 0xcc02);
}

#[test]
fn test_jsr() {
    let mut cpu = init_cpu(vec![0x20, 0x03, 0x80, 0xa9, 0x51, 0x00]);
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
    let mut cpu = init_cpu(vec![0x20, 0x04, 0x80, 0x00, 0xa9, 0x51, 0x60, 0x00]);
    cpu.run();

    assert_eq!(cpu.stack_pointer, STACK_RESET);
    assert_eq!(cpu.program_counter, 0x8004);
}

#[test]
fn test_ldx() {
    let mut cpu = init_cpu(vec![0xa2, 0x51, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_x, 0x51);
}

#[test]
fn test_ldy() {
    let mut cpu = init_cpu(vec![0xa0, 0x51, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_y, 0x51);
}

#[test]
fn test_lsr_accumulator() {
    let mut cpu = init_cpu(vec![0x4a, 0x00]);
    cpu.register_a = 0b0101_0101;
    cpu.run();

    assert_eq!(cpu.register_a, 0b0010_1010);
    assert!(cpu.status.carry_flag);
}

#[test]
fn test_ora() {
    let mut cpu = init_cpu(vec![0x09, 0b0101_0101, 0x00]);
    cpu.register_a = 0b1010_1010;
    cpu.run();

    assert_eq!(cpu.register_a, 0b1111_1111);
}

#[test]
fn test_pha() {
    let mut cpu = init_cpu(vec![0x48, 0x00]);
    cpu.register_a = 0x51;
    cpu.run();

    assert_eq!(cpu.stack_pop(), 0x51);
}

#[test]
fn test_php() {
    let mut cpu = init_cpu(vec![0x08, 0x00]);
    cpu.status.carry_flag = true;
    cpu.run();

    assert_eq!(cpu.stack_pop(), 0b0011_0101);
}

#[test]
fn test_pla() {
    let mut cpu = init_cpu(vec![0xa9, 0x51, 0x48, 0xa9, 0x50, 0x68, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_a, 0x51);
}

#[test]
fn test_plp() {
    let mut cpu = init_cpu(vec![0x08, 0x38, 0x28, 0x00]);
    cpu.run();

    assert!(!cpu.status.carry_flag);
}

#[test]
fn test_rol_accumulator() {
    let mut cpu = init_cpu(vec![0xa9, 0b1010_1010, 0x2a, 0x00]);
    cpu.status.carry_flag = true;
    cpu.run();

    assert_eq!(cpu.register_a, 0b0101_0101);
    assert!(cpu.status.carry_flag);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_rol() {
    let mut cpu = init_cpu(vec![0xa9, 0b1010_1010, 0x85, 0xc0, 0x26, 0xc0, 0x00]);
    cpu.status.carry_flag = true;
    cpu.run();

    assert_eq!(cpu.mem_read(0xc0), 0b0101_0101);
    assert!(cpu.status.carry_flag);
    assert!(!cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_ror_accumulator() {
    let mut cpu = init_cpu(vec![0xa9, 0b1010_1010, 0x6a, 0x00]);
    cpu.status.carry_flag = true;
    cpu.run();

    assert_eq!(cpu.register_a, 0b1101_0101);
    assert!(!cpu.status.carry_flag);
    assert!(cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_ror() {
    let mut cpu = init_cpu(vec![0xa9, 0b1010_1010, 0x85, 0xc0, 0x66, 0xc0, 0x00]);
    cpu.status.carry_flag = true;
    cpu.run();

    assert_eq!(cpu.mem_read(0xc0), 0b1101_0101);
    assert!(!cpu.status.carry_flag);
    assert!(cpu.status.negative_flag);
    assert!(!cpu.status.zero_flag);
}

#[test]
fn test_stx() {
    let mut cpu = init_cpu(vec![0xa9, 0x51, 0xaa, 0x86, 0xc0, 0x00]);
    cpu.run();

    assert_eq!(cpu.mem_read(0xc0), 0x51);
}

#[test]
fn test_sty() {
    let mut cpu = init_cpu(vec![0xa9, 0x51, 0xa8, 0x84, 0xc0, 0x00]);
    cpu.run();

    assert_eq!(cpu.mem_read(0xc0), 0x51);
}

#[test]
fn test_tay() {
    let mut cpu = init_cpu(vec![0xa9, 0x51, 0xa8, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_y, 0x51);
}

#[test]
fn test_tsx() {
    let mut cpu = init_cpu(vec![0xba, 0x00]);
    cpu.run();

    assert_eq!(cpu.register_x, 0xfd);
}

// TODO: write tests for Unofficial Opcodes
