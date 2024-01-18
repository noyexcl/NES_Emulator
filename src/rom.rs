const NES_TAG: [u8; 4] = [0x4e, 0x45, 0x53, 0x1a];
const PRG_ROM_PAGE_SIZE: usize = 16384;
const CHR_ROM_PAGE_SIZE: usize = 8192;

#[derive(Debug, PartialEq)]
pub enum Mirroring {
    Vertical,
    Horizontal,
    FourScreen,
}

#[derive(Debug)]
pub struct Rom {
    pub prg_rom: Vec<u8>,
    pub chr_rom: Vec<u8>,
    pub mapper: u8,
    pub screen_mirroring: Mirroring,
}

impl Rom {
    pub fn new(raw: &Vec<u8>) -> Result<Rom, String> {
        if &raw[0..4] != NES_TAG {
            return Err("File is not in iNES format.".to_string());
        }

        let mapper = raw[7] & 0b1111_0000 | raw[6] >> 4;
        let ines_ver = (raw[7] >> 2) & 0b11;
        if ines_ver != 0 {
            return Err("NES2.0 format is not supported.".to_string());
        }

        let four_screen = raw[6] & 0b1000 != 0;
        let vertical_mirroring = raw[6] & 0b1 != 0;
        let screen_mirroring = match (four_screen, vertical_mirroring) {
            (true, _) => Mirroring::FourScreen,
            (false, true) => Mirroring::Vertical,
            (false, false) => Mirroring::Horizontal,
        };

        let prg_rom_size = raw[4] as usize * PRG_ROM_PAGE_SIZE;
        let chr_rom_size = raw[5] as usize * CHR_ROM_PAGE_SIZE;

        let skip_trainer = raw[6] & 0b100 != 0;

        let prg_rom_start = 16 + if skip_trainer { 512 } else { 0 };
        let chr_rom_start = prg_rom_start + prg_rom_size;

        Ok(Rom {
            prg_rom: raw[prg_rom_start..(prg_rom_start + prg_rom_size)].to_vec(),
            chr_rom: raw[chr_rom_start..(chr_rom_start + chr_rom_size)].to_vec(),
            mapper,
            screen_mirroring,
        })
    }
}

pub mod test {
    use super::*;

    pub struct TestRom {
        header: Vec<u8>,
        trainer: Option<Vec<u8>>,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
    }

    impl TestRom {
        pub fn create_test_rom(instructions: Vec<u8>) -> Rom {
            let header = vec![
                0x4e,
                0x45,
                0x53,
                0x1a,
                0x02, // Size of PRG ROM in 16KB PRG
                0x01, // Size of CHR ROM in 8KB
                0b0011_0001,
                0b0000_0000,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
            ];

            let mut test_rom = TestRom {
                header,
                trainer: None,
                prg_rom: vec![0; 2 * PRG_ROM_PAGE_SIZE],
                chr_rom: vec![0; 1 * CHR_ROM_PAGE_SIZE],
            };

            test_rom.prg_rom[0..instructions.len()].copy_from_slice(&instructions);
            test_rom.prg_rom[0x7ffc] = 0x00;
            test_rom.prg_rom[0x7ffd] = 0x80;
            let raw = test_rom.dump();
            Rom::new(&raw).unwrap()
        }

        fn dump(&self) -> Vec<u8> {
            let mut result = Vec::with_capacity(
                self.header.len()
                    + self.trainer.as_ref().map_or(0, |t| t.len())
                    + self.prg_rom.len()
                    + self.chr_rom.len(),
            );

            result.extend(&self.header);
            if let Some(t) = &self.trainer {
                result.extend(t);
            }
            result.extend(&self.prg_rom);
            result.extend(&self.chr_rom);

            result
        }
    }

    #[test]
    fn test_new() {
        let header = vec![
            0x4e,
            0x45,
            0x53,
            0x1a,
            0x02, // Size of PRG ROM in 16KB PRG
            0x01, // Size of CHR ROM in 8KB
            0b0011_0001,
            0b0000_0000,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
        ];

        let test_rom = TestRom {
            header,
            trainer: None,
            prg_rom: vec![1; 2 * PRG_ROM_PAGE_SIZE],
            chr_rom: vec![2; 1 * CHR_ROM_PAGE_SIZE],
        };

        let raw = test_rom.dump();
        let rom = Rom::new(&raw).unwrap();

        assert_eq!(rom.screen_mirroring, Mirroring::Vertical);
        assert_eq!(rom.mapper, 0b0000_0011);
        assert_eq!(rom.prg_rom.len(), 2 * PRG_ROM_PAGE_SIZE);
        assert_eq!(rom.chr_rom.len(), 1 * CHR_ROM_PAGE_SIZE);
        assert_eq!(rom.prg_rom[0], 1);
        assert_eq!(rom.chr_rom[0], 2);
    }

    #[test]
    fn test_with_trainer() {
        let header = vec![
            0x4e,
            0x45,
            0x53,
            0x1a,
            0x02, // Size of PRG ROM in 16KB PRG
            0x01, // Size of CHR ROM in 8KB
            0b0011_0100,
            0b0000_0000,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
        ];

        let test_rom = TestRom {
            header,
            trainer: Some(vec![0; 512]),
            prg_rom: vec![1; 2 * PRG_ROM_PAGE_SIZE],
            chr_rom: vec![2; 1 * CHR_ROM_PAGE_SIZE],
        };

        let raw = test_rom.dump();
        let rom = Rom::new(&raw).unwrap();

        assert_eq!(rom.screen_mirroring, Mirroring::Horizontal);
        assert_eq!(rom.mapper, 0b0000_0011);
        assert_eq!(rom.prg_rom.len(), 2 * PRG_ROM_PAGE_SIZE);
        assert_eq!(rom.chr_rom.len(), 1 * CHR_ROM_PAGE_SIZE);
        assert_eq!(rom.prg_rom[0], 1);
        assert_eq!(rom.chr_rom[0], 2);
    }

    #[test]
    fn test_nes2() {
        let header = vec![
            0x4e,
            0x45,
            0x53,
            0x1a,
            0x02, // Size of PRG ROM in 16KB PRG
            0x01, // Size of CHR ROM in 8KB
            0b0011_0100,
            0b0000_1000,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
        ];

        let test_rom = TestRom {
            header,
            trainer: None,
            prg_rom: vec![1; 2 * PRG_ROM_PAGE_SIZE],
            chr_rom: vec![2; 1 * CHR_ROM_PAGE_SIZE],
        };

        let raw = test_rom.dump();
        let rom = Rom::new(&raw);

        rom.expect_err("NES2 should not be accepted");
    }
}
