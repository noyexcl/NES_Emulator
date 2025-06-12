pub mod registers;

use self::registers::{
    addr::AddrRegister, control::ControlRegister, mask::MaskRegister, scroll::ScrollRegister,
    status::StatusRegister,
};
use crate::{mem::Mem, rom::Mirroring, trace::Inspector};

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug)]
pub struct PPU {
    nmi_interrupt: Option<u8>,
    /// Set to true when NMI interrupt is occurred or a frame's worth of cycles has elapsed  
    pub chr_rom: Vec<u8>,
    pub chr_ram: [u8; 2048],
    pub vram: [u8; 2048],
    pub palette_table: [u8; 32],
    pub mirroring: Mirroring,
    pub ctrl: ControlRegister,
    pub mask: MaskRegister,
    pending_mask: Option<(u8, u8)>,
    pub status: StatusRegister,
    pub scroll: ScrollRegister,
    pub oam_addr: u8,
    pub oam_data: [u8; 256],
    pub addr: AddrRegister,
    internal_data_buf: u8,
    pub scanline: u16,
    pub cycles: usize,
    open_bus: u8,
    odd_frame: bool,
    suppress_vblank: bool,
}

impl PPU {
    pub fn new(chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        PPU {
            chr_rom,
            chr_ram: [0; 2048],
            palette_table: [0; 32],
            vram: [0; 2048],
            oam_data: [0; 64 * 4],
            mirroring,
            ctrl: ControlRegister::new(),
            mask: MaskRegister::new(),
            pending_mask: None,
            status: StatusRegister::new(),
            scroll: ScrollRegister::new(),
            oam_addr: 0,
            addr: AddrRegister::new(),
            internal_data_buf: 0,
            scanline: 0,
            cycles: 0,
            nmi_interrupt: None,
            open_bus: 0,
            odd_frame: false,
            suppress_vblank: false,
        }
    }

    // Horizontal:
    //   [ A ] [ A']
    //   [ B ] [ B']

    // Vertical:
    //   [ A ] [ B ]
    //   [ A'] [ B']
    pub fn mirror_vram_addr(&self, addr: u16) -> u16 {
        let mirrored_vram = addr & 0b0010_1111_1111_1111; // mirror down 0x3000-0x3eff to 0x2000-0x2eff
        let vram_index = mirrored_vram - 0x2000; // to vram vector
        let name_table = vram_index / 0x400; // to the name table index

        match (&self.mirroring, name_table) {
            (Mirroring::Vertical, 2) | (Mirroring::Vertical, 3) => vram_index - 0x800,
            (Mirroring::Horizontal, 2) => vram_index - 0x400,
            (Mirroring::Horizontal, 1) => vram_index - 0x400,
            (Mirroring::Horizontal, 3) => vram_index - 0x800,
            _ => vram_index,
        }
    }

    pub fn write_to_ppu_addr(&mut self, value: u8) {
        self.addr.update(value);
    }

    pub fn write_to_ctrl(&mut self, value: u8) {
        let before_nmi_status = self.ctrl.generate_vblank_nmi();
        self.ctrl = ControlRegister::from_bits_truncate(value);

        // Writing to CTRL register with value that enables vblank NMI while vblank flag in STATUS is 1
        // immediately trigger NMI.
        // If such writing occurs at the same time as vblank flag in STATUS is cleared (261, 0), NMI should not be triggered.
        if !before_nmi_status
            && self.ctrl.generate_vblank_nmi()
            && (self.status.is_in_vblank() && self.scanline != 261)
        {
            self.nmi_interrupt = Some(1);
        }

        // Disabling Vblank-NMI at the same time or after 1~2 cycles NMI occured supresses NMI.
        if !self.ctrl.generate_vblank_nmi() && self.scanline == 241 && self.cycles <= 2 {
            self.nmi_interrupt = None;
        }
    }

    pub fn write_to_mask(&mut self, value: u8) {
        // Toggling rendering takes effect approximately 3-4 dots after the write.
        // Other bits should be immediately applied?
        self.pending_mask = Some((2, value));
    }

    fn update_mask(&mut self) {
        if let Some((delay, value)) = self.pending_mask {
            if delay == 0 {
                self.mask = MaskRegister::from_bits_truncate(value);
                self.pending_mask = None;
            } else {
                self.pending_mask = Some((delay - 1, value));
            }
        }
    }

    pub fn write_to_scroll(&mut self, value: u8) {
        self.scroll.write(value);
    }

    pub fn write_to_oam_addr(&mut self, value: u8) {
        self.oam_addr = value;
    }

    pub fn write_to_oam_data(&mut self, value: u8) {
        self.oam_data[self.oam_addr as usize] = value;
        self.oam_addr = self.oam_addr.wrapping_add(1);
    }

    pub fn read_oam_data(&self) -> u8 {
        self.oam_data[self.oam_addr as usize]
    }

    pub fn write_oam_dma(&mut self, data: &[u8; 256]) {
        for x in data.iter() {
            self.oam_data[self.oam_addr as usize] = *x;
            self.oam_addr = self.oam_addr.wrapping_add(1);
        }
    }

    pub fn read_status(&mut self) -> u8 {
        let value = self.status.bits();
        self.status.reset_vblank_status();
        self.addr.reset_latch();
        self.scroll.reset_latch();

        /* Reading $2002 within a few PPU clocks of when VBL is set results in special-case behavior. */

        // Reading one PPU clock before reads it as clear and never sets the flag or generates NMI for that frame.
        if self.scanline == 241 && self.cycles == 0 {
            self.suppress_vblank = true;
            self.nmi_interrupt = None;
            0
        }
        // Reading on the same PPU clock or one later reads it as set, clears it, and suppresses the NMI for that frame.
        else if self.scanline == 241 && (self.cycles == 1 || self.cycles == 2) {
            self.nmi_interrupt = None;
            value
        }
        // Reading two or more PPU clocks before/after it's set behaves normally (reads flag's value, clears it, and doesn't affect NMI operation).
        else {
            value
        }
    }

    fn increment_vram_addr(&mut self) {
        self.addr.increment(self.ctrl.vram_addr_increment());
    }

    pub fn write_data(&mut self, value: u8) {
        let addr = self.addr.get();

        match addr {
            0..=0x1fff => {
                // panic!("attempt to write to chr_rom(addr space 0..0x1fff). it's read only. requested = {:x}", addr)
                // self.chr_rom[addr as usize] = value;
                if self.chr_rom.is_empty() {
                    self.chr_ram[addr as usize] = value;
                } else {
                    eprintln!("attempt to write to chr_rom(addr space 0..0x1fff). it's read only. requested = {:x}", addr)
                }
            }
            0x2000..=0x2fff => {
                self.vram[self.mirror_vram_addr(addr) as usize] = value;
            }
            0x3000..=0x3eff => {
                panic!(
                    "addr space 0x3000..0x3eff is not expected to be used, requested = {:x}",
                    addr
                )
            }
            /*
            0x3f10 | 0x3f14 | 0x3f18 | 0x3f1c => {
                let add_mirror = addr - 0x10;
                self.palette_table[(add_mirror - 0x3f00) as usize] = value;
            }
            0x3f00..=0x3f1f => self.palette_table[(addr - 0x3f00) as usize] = value,
            */
            0x3f00..=0x3fff => {
                let mut addr_mirrored = addr & 0x1f;

                if let 0x10 | 0x14 | 0x18 | 0x1c = addr_mirrored {
                    // Addresses $xx10/$xx14/$xx18/$xx1C in (0x3f00~0x3fff) are mirrors of $xx00/$xx04/$xx08/$xx0C
                    // because sprite and background share that byte.
                    addr_mirrored -= 0x10;
                }

                self.palette_table[addr_mirrored as usize] = value;
            }
            _ => panic!("unexpected access to mirrored space = {:x}", addr),
        }

        self.increment_vram_addr();
    }

    pub fn read_data(&mut self) -> u8 {
        let addr = self.addr.get();
        self.increment_vram_addr();

        match addr {
            0..=0x1fff => {
                if self.chr_rom.is_empty() {
                    let result = self.internal_data_buf;
                    self.internal_data_buf = self.chr_ram[addr as usize];
                    result
                } else {
                    let result = self.internal_data_buf;
                    self.internal_data_buf = self.chr_rom[addr as usize];
                    result
                }
            }
            0x2000..=0x2fff => {
                let result = self.internal_data_buf;
                self.internal_data_buf = self.vram[self.mirror_vram_addr(addr) as usize];
                result
            }
            0x3000..=0x3eff => panic!(
                "addr space 0x3000..0x3eff is not expected to be used, requested = {:x}",
                addr
            ),

            //Addresses $3F10/$3F14/$3F18/$3F1C are mirrors of $3F00/$3F04/$3F08/$3F0C
            /*
            0x3f10 | 0x3f14 | 0x3f18 | 0x3f1c => {
                let addr_mirror = addr - 0x10;
                self.palette_table[(addr_mirror - 0x3f00) as usize]
            }
            */
            // 0x3f00..=0x3f1f => self.palette_table[(addr - 0x3f00) as usize],
            0x3f00..=0x3fff => {
                let mut addr_mirrored = addr & 0x1f;

                if let 0x10 | 0x14 | 0x18 | 0x1c = addr_mirrored {
                    // Addresses $xx10/$xx14/$xx18/$xx1C in (0x3f00~0x3fff) are mirrors of $xx00/$xx04/$xx08/$xx0C
                    // because sprite and background share that byte.
                    addr_mirrored -= 0x10;
                }

                self.palette_table[addr_mirrored as usize]
            }
            _ => panic!("unexpected access to mirrored space = {:x}", addr),
        }
    }

    pub fn get_tile_data(&self, bank: u16, tile_index: u16) -> &[u8] {
        if self.chr_rom.is_empty() {
            &self.chr_ram
                [(bank + tile_index * 16) as usize..=(bank + tile_index * 16 + 15) as usize]
        } else {
            &self.chr_rom
                [(bank + tile_index * 16) as usize..=(bank + tile_index * 16 + 15) as usize]
        }
    }

    pub fn tick(&mut self) -> bool {
        let mut frame_ready = false;

        self.update_mask();

        match (self.scanline, self.cycles) {
            (0..=239, 0) => {
                // Idle cycle
            }
            (0..=239, 1..=256) => {
                // Fetch data
            }
            (0..=239, 257..=320) => {
                // Fetch tile data for sprites on the next scanline. every step takes 2 cycles (8 cycles in total).
                // 1. Garbage nametable byte
                // 2. Garbage nametable byte
                // 3. Pattern table tile low
                // 4. Pattern table tile high (pattern table low + 8 bytes)
            }
            (0..=239, 321..=336) => {
                // Fetch first 2 tiles for the next scanline. every step takes 2 cycles.
                // 1. Nametable byte
                // 2. Attribute table byte
                // 3. Pattern table tile low
                // 4. Pattern table tile hight (pattern table low + 8 bytes)
            }
            (0..=239, 337..=340) => {
                // Fetch 2 bytes (dummy). every step takes 2 cycles (4 cycles in total).
                // 1. Nametable byte
                // 2. Nametable byte
            }
            (240, 340) => {
                if self.ctrl.generate_vblank_nmi() {
                    self.nmi_interrupt = Some(1);
                }

                frame_ready = true;
            }
            (241, 0) => {
                if !self.suppress_vblank {
                    self.status.set_vblank_status(true);
                }

                self.suppress_vblank = false;
                self.status.set_sprite_zero_hit(false); // Is this appropriate?
            }
            (241..=260, _) => {
                // PPU Makes no memory access during these scanlines.
            }
            (261, 0) => {
                self.status.reset_vblank_status();
                self.status.set_sprite_zero_hit(false);
                self.status.set_sprite_overflow(false);
                //self.nmi_interrupt = None;
            }
            (261, _) => {
                // Pre-render scanline
            }
            _ => (),
        }

        self.cycles += 1;

        if self.cycles > 340 || (self.cycles == 340 && self.should_skip_last_tick()) {
            // Is this appropriate?
            if self.is_sprite_0_hit(self.cycles) {
                self.status.set_sprite_zero_hit(true);
            }

            self.cycles = 0;
            self.scanline += 1;
        }

        if self.scanline > 261 {
            self.scanline = 0;
            self.odd_frame = !self.odd_frame;
        }

        frame_ready
    }

    fn is_sprite_0_hit(&self, cycle: usize) -> bool {
        let y = self.oam_data[0] as usize;
        let x = self.oam_data[3] as usize;
        (y == self.scanline as usize) && x <= cycle && self.mask.show_sprite()
    }

    fn should_skip_last_tick(&self) -> bool {
        self.scanline == 261
            && self.odd_frame
            && (self.mask.contains(MaskRegister::SHOW_BACKGROUND)
                || self.mask.contains(MaskRegister::SHOW_BACKGROUND))
    }

    pub fn poll_nmi_interrupt(&mut self) -> bool {
        if let Some(delay) = self.nmi_interrupt.take() {
            if delay == 0 {
                return true;
            } else {
                self.nmi_interrupt = Some(delay - 1);
            }
        }

        false
    }
}

impl Mem for PPU {
    fn mem_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x2000 | 0x2001 | 0x2003 | 0x2005 | 0x2006 => self.open_bus,
            0x2002 => {
                let data = self.read_status();

                if self.scanline == 240 && self.cycles == 340 {
                    self.suppress_vblank = true;
                    0
                } else {
                    data
                }

                /*
                self.open_bus = (self.open_bus & 0b0001_1111) | (data & 0b1110_0000);
                self.open_bus
                */
            }
            0x2004 => {
                let data = self.read_oam_data();
                self.open_bus = data;
                self.open_bus
            }
            0x2007 => {
                let data = self.read_data();
                self.open_bus = data;
                self.open_bus
            }
            _ => unreachable!("Addr {:04X} is not PPU region", addr),
        }
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        match addr {
            0x2000 => {
                self.write_to_ctrl(data);
                self.open_bus = data;
            }
            0x2001 => {
                self.write_to_mask(data);
                self.open_bus = data;
            }
            0x2002 => {
                self.open_bus = data;
            }
            0x2003 => {
                self.write_to_oam_addr(data);
            }
            0x2004 => {
                self.write_to_oam_data(data);
            }
            0x2005 => {
                self.write_to_scroll(data);
                self.open_bus = data;
            }
            0x2006 => {
                self.write_to_ppu_addr(data);
                self.open_bus = data;
            }
            0x2007 => {
                self.write_data(data);
                self.open_bus = data;
            }
            _ => unreachable!("Addr {:04X} is not PPU region", addr),
        }
    }
}

impl Inspector for PPU {
    fn inspect(&self, addr: u16) -> u8 {
        match addr {
            0x2000 | 0x2001 | 0x2003 | 0x2005 | 0x2006 | 0x2007 => self.open_bus,
            0x2002 => (self.status.bits() & 0b1110_0000) | (self.open_bus & 0b0001_1111),
            0x2004 => self.read_oam_data(),
            _ => panic!("{:X} is not a PPU region", addr),
        }
    }

    #[allow(unused)]
    fn inspect_u16(&self, addr: u16) -> u16 {
        panic!("PPU Inspector does not support u16 read");
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    #[test]
    fn test_ppu_vram_writes() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.write_to_ppu_addr(0x23);
        ppu.write_to_ppu_addr(0x05);
        ppu.write_data(0x66);

        assert_eq!(ppu.vram[0x0305], 0x66);
    }

    #[test]
    fn test_ppu_vram_reads() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.write_to_ctrl(0);
        ppu.vram[0x0305] = 0x66;

        ppu.write_to_ppu_addr(0x23);
        ppu.write_to_ppu_addr(0x05);

        ppu.read_data(); //load_into_buffer
        assert_eq!(ppu.addr.get(), 0x2306);
        assert_eq!(ppu.read_data(), 0x66);
    }

    #[test]
    fn test_ppu_vram_reads_cross_page() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.write_to_ctrl(0);
        ppu.vram[0x01ff] = 0x66;
        ppu.vram[0x0200] = 0x77;

        ppu.write_to_ppu_addr(0x21);
        ppu.write_to_ppu_addr(0xff);

        ppu.read_data(); //load_into_buffer
        assert_eq!(ppu.read_data(), 0x66);
        assert_eq!(ppu.read_data(), 0x77);
    }

    #[test]
    fn test_ppu_vram_reads_step_32() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.write_to_ctrl(0b100);
        ppu.vram[0x01ff] = 0x66;
        ppu.vram[0x01ff + 32] = 0x77;
        ppu.vram[0x01ff + 64] = 0x88;

        ppu.write_to_ppu_addr(0x21);
        ppu.write_to_ppu_addr(0xff);

        ppu.read_data(); //load_into_buffer
        assert_eq!(ppu.read_data(), 0x66);
        assert_eq!(ppu.read_data(), 0x77);
        assert_eq!(ppu.read_data(), 0x88);
    }

    // Horizontal: https://wiki.nesdev.com/w/index.php/Mirroring
    //   [0x2000 A ] [0x2400 a ]
    //   [0x2800 B ] [0x2C00 b ]
    #[test]
    fn test_vram_horizontal_mirror() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.write_to_ppu_addr(0x24);
        ppu.write_to_ppu_addr(0x05);

        ppu.write_data(0x66); //write to a

        ppu.write_to_ppu_addr(0x28);
        ppu.write_to_ppu_addr(0x05);

        ppu.write_data(0x77); //write to B

        ppu.write_to_ppu_addr(0x20);
        ppu.write_to_ppu_addr(0x05);

        ppu.read_data(); //load into buffer
        assert_eq!(ppu.read_data(), 0x66); //read from A

        ppu.write_to_ppu_addr(0x2C);
        ppu.write_to_ppu_addr(0x05);

        ppu.read_data(); //load into buffer
        assert_eq!(ppu.read_data(), 0x77); //read from b
    }

    // Vertical: https://wiki.nesdev.com/w/index.php/Mirroring
    //   [0x2000 A ] [0x2400 B ]
    //   [0x2800 a ] [0x2C00 b ]
    #[test]
    fn test_vram_vertical_mirror() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Vertical);

        ppu.write_to_ppu_addr(0x20);
        ppu.write_to_ppu_addr(0x05);

        ppu.write_data(0x66); //write to A

        ppu.write_to_ppu_addr(0x2C);
        ppu.write_to_ppu_addr(0x05);

        ppu.write_data(0x77); //write to b

        ppu.write_to_ppu_addr(0x28);
        ppu.write_to_ppu_addr(0x05);

        ppu.read_data(); //load into buffer
        assert_eq!(ppu.read_data(), 0x66); //read from a

        ppu.write_to_ppu_addr(0x24);
        ppu.write_to_ppu_addr(0x05);

        ppu.read_data(); //load into buffer
        assert_eq!(ppu.read_data(), 0x77); //read from B
    }

    #[test]
    fn test_read_status_resets_latch() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.vram[0x0305] = 0x66;

        ppu.write_to_ppu_addr(0x21);
        ppu.write_to_ppu_addr(0x23);
        ppu.write_to_ppu_addr(0x05);

        ppu.read_data(); //load_into_buffer
        assert_ne!(ppu.read_data(), 0x66);

        ppu.read_status();

        ppu.write_to_ppu_addr(0x23);
        ppu.write_to_ppu_addr(0x05);

        ppu.read_data(); //load_into_buffer
        assert_eq!(ppu.read_data(), 0x66);
    }

    #[test]
    fn test_ppu_vram_mirroring() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.write_to_ctrl(0);
        ppu.vram[0x0305] = 0x66;

        ppu.write_to_ppu_addr(0x63); //0x6305 -> 0x2305
        ppu.write_to_ppu_addr(0x05);

        ppu.read_data(); //load into_buffer
        assert_eq!(ppu.read_data(), 0x66);
        // assert_eq!(ppu.addr.read(), 0x0306)
    }

    #[test]
    fn test_read_status_resets_vblank() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.status.set_vblank_status(true);

        let status = ppu.read_status();

        assert_eq!(status >> 7, 1);
        assert_eq!(ppu.status.bits() >> 7, 0);
    }

    #[test]
    fn test_oam_read_write() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.write_to_oam_addr(0x10);
        ppu.write_to_oam_data(0x66);
        ppu.write_to_oam_data(0x77);

        ppu.write_to_oam_addr(0x10);
        assert_eq!(ppu.read_oam_data(), 0x66);

        ppu.write_to_oam_addr(0x11);
        assert_eq!(ppu.read_oam_data(), 0x77);
    }

    #[test]
    fn test_oam_dma() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);

        let mut data = [0x66; 256];
        data[0] = 0x77;
        data[255] = 0x88;

        ppu.write_to_oam_addr(0x10);
        ppu.write_oam_dma(&data);

        ppu.write_to_oam_addr(0xf); //wrap around
        assert_eq!(ppu.read_oam_data(), 0x88);

        ppu.write_to_oam_addr(0x10);
        assert_eq!(ppu.read_oam_data(), 0x77);

        ppu.write_to_oam_addr(0x11);
        assert_eq!(ppu.read_oam_data(), 0x66);
    }
}
