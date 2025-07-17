#![allow(clippy::unusual_byte_groupings)]

pub mod frame;
pub mod palette;
pub mod registers;

use self::registers::{PPUCTRL, PPUMASK, PPUSTATUS};
use crate::{rom::Mirroring, trace::Inspector};
use frame::Frame;

#[derive(Debug)]
struct Sprite {
    x: usize,
    y: usize,
    flip_vertical: bool,
    flip_horizontal: bool,
    tile_idx: usize,
    palette_idx: usize,
    is_zero: bool,
}

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
    ctrl: PPUCTRL,
    mask: PPUMASK,
    pending_mask: Option<(u8, u8)>,
    status: PPUSTATUS,
    oam_addr: u8,
    oam_data: [u8; 256],
    secandary_oam: Vec<Sprite>,
    data_buffer: u8,
    pub scanline: usize,
    pub cycles: usize,
    open_bus: u8,
    odd_frame: bool,
    suppress_vblank: bool,
    pub frame: Frame,
    /// Current VRAM address (composed of scroll, nametable).
    /// ```text
    /// yyy NN YYYYY XXXXX
    /// ||| || ||||| +++++-- coarse X scroll
    /// ||| || +++++-------- coarse Y scroll
    /// ||| ++-------------- nametable select
    /// +++----------------- fine Y scroll
    /// ```
    v: u16,
    /// Temporary VRAM address. \
    /// Changes to scroll, nametable are stored this register temporarily then applied to v at a specific taiming (v=t).
    t: u16,
    /// Pixel base x offset in a tile.
    x: u8,
    /// Temporary fine x offset in a tile.
    t_x: u8,
    /// latch of ADDR and SCROLL registers to toggle hi/lo and x/y.
    w: bool,
}

impl PPU {
    pub fn new(chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        PPU {
            chr_rom,
            chr_ram: [0; 2048],
            palette_table: [0; 32],
            vram: [0; 2048],
            oam_data: [0; 64 * 4],
            secandary_oam: Vec::with_capacity(8),
            mirroring,
            ctrl: PPUCTRL::new(),
            mask: PPUMASK::new(),
            pending_mask: None,
            status: PPUSTATUS::new(),
            oam_addr: 0,
            data_buffer: 0,
            scanline: 0,
            cycles: 0,
            nmi_interrupt: None,
            open_bus: 0,
            odd_frame: false,
            suppress_vblank: false,
            frame: Frame::new(),
            v: 0,
            t: 0,
            x: 0,
            t_x: 0,
            w: false,
        }
    }

    fn mirror_vram_addr(&self, addr: u16) -> u16 {
        let addr = addr & 0b0010_1111_1111_1111; // mirror down 0x3000-0x3eff to 0x2000-0x2eff
        let addr = addr - 0x2000;
        let idx = addr / 0x400;

        // Horizontal:
        //   [ A ] [ A']   A,A' = 0x000, B,B' = 0x400
        //   [ B ] [ B']
        //
        // Vertical:
        //   [ A ] [ B ]
        //   [ A'] [ B']
        match (&self.mirroring, idx) {
            (Mirroring::Horizontal, 1) => addr - 0x400,
            (Mirroring::Horizontal, 2) => addr - 0x400,
            (Mirroring::Horizontal, 3) => addr - 0x800,
            (Mirroring::Vertical, 2) => addr - 0x800,
            (Mirroring::Vertical, 3) => addr - 0x800,

            _ => addr,
        }
    }

    fn write_to_ctrl(&mut self, value: u8) {
        let before_nmi_status = self.ctrl.generate_vblank_nmi();
        self.ctrl = PPUCTRL::from_bits_truncate(value);

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

    fn write_to_mask(&mut self, value: u8) {
        // Toggling rendering takes effect approximately 3-4 dots after the write.
        // Other bits should be immediately applied?
        self.pending_mask = Some((2, value));
    }

    fn update_mask(&mut self) {
        if let Some((delay, value)) = self.pending_mask {
            if delay == 0 {
                self.mask = PPUMASK::from_bits_truncate(value);
                self.pending_mask = None;
            } else {
                self.pending_mask = Some((delay - 1, value));
            }
        }
    }

    fn write_to_oam_addr(&mut self, value: u8) {
        self.oam_addr = value;
    }

    fn write_to_oam_data(&mut self, value: u8) {
        self.oam_data[self.oam_addr as usize] = value;
        self.oam_addr = self.oam_addr.wrapping_add(1);
    }

    fn read_oam_data(&self) -> u8 {
        self.oam_data[self.oam_addr as usize]
    }

    pub fn write_oam_dma(&mut self, data: &[u8; 256]) {
        for x in data.iter() {
            self.oam_data[self.oam_addr as usize] = *x;
            self.oam_addr = self.oam_addr.wrapping_add(1);
        }
    }

    fn read_status(&mut self) -> u8 {
        let value = self.status.bits();
        self.status.reset_vblank_status();

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

    #[allow(clippy::needless_range_loop)]
    fn get_tile(&self, bank_addr: u16, idx: u8) -> [u8; 16] {
        let mut tile = [0u8; 16];

        for i in 0..16 {
            tile[i] = self.mem_read((bank_addr + idx as u16 * 16) + i as u16);
        }

        tile
    }

    fn get_bg_palette(&self, attr: u8, tile_x: u8, tile_y: u8) -> [u8; 4] {
        let palette_idx = match (tile_x % 4 / 2, tile_y % 4 / 2) {
            (0, 0) => attr & 0b11,
            (1, 0) => (attr >> 2) & 0b11,
            (0, 1) => (attr >> 4) & 0b11,
            (1, 1) => (attr >> 6) & 0b11,
            _ => unreachable!(),
        };

        let palette_start: usize = 1 + (palette_idx as usize) * 4;

        [
            self.palette_table[0],
            self.palette_table[palette_start],
            self.palette_table[palette_start + 1],
            self.palette_table[palette_start + 2],
        ]
    }

    fn get_sp_palette(&self, idx: usize) -> [u8; 4] {
        let start = 0x11 + idx * 4;
        [
            0,
            self.palette_table[start],
            self.palette_table[start + 1],
            self.palette_table[start + 2],
        ]
    }

    fn get_color_idx(&self, tile: [u8; 16], offset_x: usize, offset_y: usize) -> usize {
        // Tile consists of 2 separate tables(8x8 *2).
        // Each bit of the tables corresponds the lower/upper value of the dot which represents palette idx.
        //
        // Lower bits         Upper bits
        // 0x0000: 11100000   0x0008: 00000000
        // 0x0001: 11000000   0x0009: 00100000
        // ...                ...
        // 0x0007: 0000000   0x000F: 000011111
        //
        // Combine these bits, compute the actual palette idx
        // For example, top left dot of the tile is 01 and right bottom dot is 10.
        // See: https://bugzmanov.github.io/nes_ebook/chapter_6_3.html for more details

        let lo = (tile[offset_y] << offset_x) & 0b1000_0000 != 0;
        let hi = (tile[offset_y + 8] << offset_x) & 0b1000_0000 != 0;
        (hi as usize) << 1 | lo as usize
    }

    /// Render backgrournd dot and return if the pixel is opaque
    fn render_bg_dot(&mut self, x: usize, y: usize) -> bool {
        let tile_addr = 0x2000 | (self.v & 0x0FFF);
        let tile_idx = self.mem_read(tile_addr);
        let tile = self.get_tile(self.ctrl.background_pattern_addr(), tile_idx);

        let attr = self
            .mem_read(0x23C0 | (self.v & 0x0C00) | ((self.v >> 4) & 0x38) | ((self.v >> 2) & 0x07));

        let palette = self.get_bg_palette(attr, self.tile_x(), self.tile_y());

        let color_idx = self.get_color_idx(tile, self.fine_x() as usize, self.fine_y() as usize);
        let color = palette[color_idx];

        self.frame.set_pixel(x, y, color);

        color_idx != 0x00
    }

    /// Render the first registered sprite dot that is opaque if there are any. \
    /// Return if it actually rendered.
    fn render_sp_dot(&mut self, x: usize, y: usize) -> bool {
        let mut sprite_0_hit = false;

        // We just render every sprites in reverse to display the first opaque one.
        for sprite in self.secandary_oam.iter().rev() {
            if sprite.x <= x && x - sprite.x < 8 {
                let tile = self.get_tile(self.ctrl.sprite_pattern_addr(), sprite.tile_idx as u8);
                let palette = self.get_sp_palette(sprite.palette_idx);

                let offset_x = if sprite.flip_horizontal {
                    ((x - sprite.x) as isize - 7).unsigned_abs()
                } else {
                    x - sprite.x
                };

                let offset_y = if sprite.flip_vertical {
                    ((y - sprite.y) as isize - 7).unsigned_abs()
                } else {
                    y - sprite.y
                };

                let color_idx = self.get_color_idx(tile, offset_x, offset_y);
                let color = palette[color_idx];

                if color_idx != 0x00 {
                    self.frame.set_pixel(x, y, color);

                    if sprite.is_zero {
                        sprite_0_hit = true;
                    }
                }
            }
        }

        sprite_0_hit
    }

    fn find_sprites_on(&mut self, line: usize) {
        for i in (0..self.oam_data.len()).step_by(4) {
            let y = self.oam_data[i] as usize;

            if y <= line && line - y < 8 {
                if self.secandary_oam.len() == 8 {
                    // self.status.set_sprite_overflow(true);
                    break;
                }

                self.secandary_oam.push(Sprite {
                    x: self.oam_data[i + 3] as usize,
                    y: y + 1,
                    tile_idx: self.oam_data[i + 1] as usize,
                    flip_vertical: self.oam_data[i + 2] >> 7 & 1 == 1,
                    flip_horizontal: self.oam_data[i + 2] >> 6 & 1 == 1,
                    palette_idx: (self.oam_data[i + 2] & 0b11) as usize,
                    is_zero: i == 0,
                });
            }
        }
    }

    pub fn tick(&mut self) -> bool {
        let mut frame_ready = false;

        self.update_mask();

        match (self.scanline, self.cycles) {
            (0..=239, 0) => {
                // Actually this is an idle cycle.
                // But we evaluate sprites here to render on this line. (it should be done on the previous line.)
                self.secandary_oam.clear();

                if self.scanline != 0 {
                    self.find_sprites_on(self.scanline - 1);
                }
            }
            (0..=239, 1..=256) => {
                let bg_rendered = if self.mask.contains(PPUMASK::SHOW_BACKGROUND) {
                    self.render_bg_dot(self.cycles - 1, self.scanline)
                } else {
                    false
                };

                let sp_rendered = if self.mask.contains(PPUMASK::SHOW_SPRITES) {
                    self.render_sp_dot(self.cycles - 1, self.scanline)
                } else {
                    false
                };

                if sp_rendered
                    && self
                        .mask
                        .contains(PPUMASK::SHOW_SPRITES | PPUMASK::SHOW_BACKGROUND)
                    && (!(1..=8).contains(&self.cycles)
                        || (self
                            .mask
                            .contains(PPUMASK::LEFTMOST_BG | PPUMASK::LEFTMOST_SP)))
                    && self.cycles != 256
                {
                    self.status.set_sprite_zero_hit(true);
                }

                if self
                    .mask
                    .intersects(PPUMASK::SHOW_BACKGROUND | PPUMASK::SHOW_SPRITES)
                {
                    self.inc_x();

                    if self.cycles == 256 {
                        self.inc_y();
                    }
                }
            }
            (0..=239, 257) => {
                if self
                    .mask
                    .intersects(PPUMASK::SHOW_BACKGROUND | PPUMASK::SHOW_SPRITES)
                {
                    // hori(v) = hori(t)
                    // v: ... .A ..... BCDEF <- t: ... .A ..... BCDEF
                    let mask = 0b000_01_00000_11111;
                    self.v = self.v & !mask | self.t & mask;
                    self.x = self.t_x;
                }
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
            }
            (241..=260, _) => {
                // PPU Makes no memory access during these scanlines.
            }
            (261, 0) => {
                self.status.reset_vblank_status();
                self.status.set_sprite_zero_hit(false);
                self.status.set_sprite_overflow(false);
            }
            (261, 257) => {}
            (261, 280..=304) => {
                if self
                    .mask
                    .intersects(PPUMASK::SHOW_BACKGROUND | PPUMASK::SHOW_SPRITES)
                {
                    // v: GHI A. BCDEF ..... <- t: GHI A. BCDEF .....
                    let mask = 0b111_10_11111_00000;
                    self.v = self.v & !mask | self.t & mask;
                }
            }
            _ => (),
        }

        self.cycles += 1;

        if self.cycles > 340 || (self.cycles == 340 && self.should_skip_last_tick()) {
            self.cycles = 0;
            self.scanline += 1;
        }

        if self.scanline > 261 {
            self.scanline = 0;
            self.odd_frame = !self.odd_frame;
        }

        frame_ready
    }

    fn should_skip_last_tick(&self) -> bool {
        self.scanline == 261 && self.odd_frame && self.mask.contains(PPUMASK::SHOW_BACKGROUND)
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

    fn tile_x(&self) -> u8 {
        (self.v & 0x1F) as u8
    }

    fn tile_y(&self) -> u8 {
        ((self.v & 0x03E0) >> 5) as u8
    }

    fn fine_x(&self) -> u8 {
        self.x
    }

    fn fine_y(&self) -> u8 {
        ((self.v & 0x7000) >> 12) as u8
    }

    fn inc_x(&mut self) {
        if self.x == 7 {
            self.x = 0;
            self.inc_tile_x();
        } else {
            self.x += 1;
        }
    }

    /// Increment tile X
    fn inc_tile_x(&mut self) {
        // if tile_x == 31
        if (self.v & 0x001F) == 31 {
            self.v &= !0x001F; // tile_x = 0
            self.v ^= 0x400; // Switch horizontal nametable
        } else {
            self.v += 1;
        }
    }

    /// Increment Y
    fn inc_y(&mut self) {
        // If fine_y < 7, which means it should stay inside the same tile but just scroll 1 pixel-line down.
        if (self.v & 0x7000) != 0x7000 {
            self.v += 0x1000; // Increment fine_y
        }
        // If fine_y >= 7, it needs to move down to the next tile-line.
        else {
            self.v &= !0x7000; // fine_y = 0

            let mut tile_y = (self.v & 0x03E0) >> 5; // current tile_y

            // 29 is the last row of tiles in a nametable
            if tile_y == 29 {
                tile_y = 0;
                self.v ^= 0x0800; // Switch vertical nametable

            // tile_y can be set out of bounds (>29). tiles stored there are attribute data.
            // If tile_y is incremented from 31, it will wrap to 0 in the same table.
            } else if tile_y == 31 {
                tile_y = 0; // nametable not switched
            } else {
                tile_y += 1 // Increment tile_y
            }

            self.v = (self.v & !0x03E0) | (tile_y << 5)
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0..=0x1fff => {
                if self.chr_rom.is_empty() {
                    self.chr_ram[addr as usize]
                } else {
                    self.chr_rom[addr as usize]
                }
            }
            // Name and attribute table
            0x2000..=0x3eff => self.vram[self.mirror_vram_addr(addr) as usize],
            // Palette table
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

    fn mem_write(&mut self, addr: u16, data: u8) {
        match addr {
            0..=0x1fff => {
                if self.chr_rom.is_empty() {
                    self.chr_ram[addr as usize] = data;
                } else {
                    eprintln!("attempt to write to chr_rom(addr space 0..0x1fff). it's read only. requested = {:x}", addr)
                }
            }
            // Name and attribute table
            0x2000..=0x3eff => {
                self.vram[self.mirror_vram_addr(addr) as usize] = data;
            }
            // Palette table
            0x3f00..=0x3fff => {
                let mut addr_mirrored = addr & 0x1f;

                if let 0x10 | 0x14 | 0x18 | 0x1c = addr_mirrored {
                    // Addresses $xx10/$xx14/$xx18/$xx1C in (0x3f00~0x3fff) are mirrors of $xx00/$xx04/$xx08/$xx0C.
                    // these bytes indicate bg color and background and sprite palettes share them.
                    // so sprite's bg color(e.g. $3F10) should mirror down to background's bg color(e.g. $3F00).
                    addr_mirrored -= 0x10;
                }

                self.palette_table[addr_mirrored as usize] = data;
            }
            _ => panic!("unexpected access to mirrored space = {:x}", addr),
        }
    }

    pub fn read_port(&mut self, addr: u16) -> u8 {
        match addr {
            0x2000 | 0x2001 | 0x2003 | 0x2005 | 0x2006 => self.open_bus,
            0x2002 => {
                self.w = false;
                self.read_status()
            }
            0x2004 => {
                let data = self.read_oam_data();
                self.open_bus = data;
                self.open_bus
            }
            0x2007 => {
                let result = self.data_buffer;
                self.data_buffer = self.mem_read(self.v & 0x3FFF);
                self.v += self.ctrl.vram_addr_increment() as u16;
                // self.v = self.v.wrapping_add(self.ctrl.vram_addr_increment() as u16);

                result
            }
            _ => unreachable!("Addr {:04X} is not PPU region", addr),
        }
    }

    pub fn write_to_port(&mut self, addr: u16, data: u8) {
        match addr {
            0x2000 => {
                // t: ...GH.. ........ <- d: ......GH
                //    <used elsewhere> <- d: ABCDEF..
                self.t = self.t & 0b1110011_11111111 | (data as u16 & 0b11) << 10;

                self.write_to_ctrl(data);

                self.open_bus = data;
            }
            0x2001 => {
                self.write_to_mask(data);
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
                if self.w {
                    // t: FGH .. ABCDE ..... <- d: ABCDEFGH
                    let fine_y = data & 0b111;
                    let tile_y = (data & 0b1111_1000) >> 3;

                    self.t = (self.t & 0b000_11_00000_11111)
                        | (fine_y as u16) << 12
                        | (tile_y as u16) << 5;
                } else {
                    // t: ....... ...ABCDE <- d: ABCDE...
                    // x:              FGH <- d: .....FGH
                    let tile_x = (data & 0b1111_1000) >> 3;
                    let fine_x = data & 0b111;

                    self.t = self.t & 0b111_11_11111_00000 | tile_x as u16;
                    self.t_x = fine_x;
                }

                self.w = !self.w;
            }
            0x2006 => {
                if !self.w {
                    // t: .CDEFGH ........ <- d: ..CDEFGH
                    //        <unused>     <- d: AB......
                    // t: Z...... ........ <- 0 (bit Z is cleared)
                    let hi_6bits = data & 0b0011_1111;
                    self.t = self.t & 0b0_00_0000_1111_1111 | (hi_6bits as u16) << 8;
                } else {
                    // t: ....... ABCDEFGH <- d: ABCDEFGH
                    // v: <...all bits...> <- t: <...all bits...>
                    self.t = self.t & 0b1111_1111_0000_0000 | data as u16;
                    self.v = self.t;
                }

                self.w = !self.w;
            }
            0x2007 => {
                self.mem_write(self.v & 0x3FFF, data);
                self.v += self.ctrl.vram_addr_increment() as u16;
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
        ppu.write_to_port(0x2006, 0x23);
        ppu.write_to_port(0x2006, 0x05);
        ppu.write_to_port(0x2007, 0x66); // write 0x66 to 0x2305 (in vram address, it's 0x0305)

        assert_eq!(ppu.vram[0x0305], 0x66); // read 0x66 from 0x2305
    }

    #[test]
    fn test_ppu_vram_reads() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.write_to_port(0x2000, 0);
        ppu.vram[0x0305] = 0x66; // write 0x66 to 0x2305

        ppu.write_to_port(0x2006, 0x23);
        ppu.write_to_port(0x2006, 0x05);
        let _ = ppu.read_port(0x2007); // Load value at 0x2305 into buffer
        let data = ppu.read_port(0x2007);
        assert_eq!(data, 0x66);
    }

    #[test]
    fn test_ppu_vram_reads_cross_page() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.write_to_port(0x2000, 0);
        ppu.vram[0x01ff] = 0x66; // write 0x66 to 0x21ff
        ppu.vram[0x0200] = 0x77; // write 0x77 to 0x2200

        ppu.write_to_port(0x2006, 0x21);
        ppu.write_to_port(0x2006, 0xff);

        let _ = ppu.read_port(0x2007); // Load value at 0x21ff into buffer and increment vram address (it now should be pointing to 0x2200)
        let data = ppu.read_port(0x2007); // Get value in buffer (value at 0x21ff)  and load value at 0x2200 into buffer
        assert_eq!(data, 0x66);

        let data = ppu.read_port(0x2007); // Get value in buffer (value at 0x2200)
        assert_eq!(data, 0x77);
    }

    #[test]
    fn test_ppu_vram_reads_step_32() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.write_to_port(0x2000, 0b100);
        ppu.vram[0x01ff] = 0x66; // write 0x66 to 0x21ff
        ppu.vram[0x01ff + 32] = 0x77; // write 0x77 to 0x221f
        ppu.vram[0x01ff + 64] = 0x88; // write 0x88 to 0x223f

        ppu.write_to_port(0x2006, 0x21);
        ppu.write_to_port(0x2006, 0xff);

        let _ = ppu.read_port(0x2007);
        let data = ppu.read_port(0x2007);
        assert_eq!(data, 0x66);

        let data = ppu.read_port(0x2007);
        assert_eq!(data, 0x77);

        let data = ppu.read_port(0x2007);
        assert_eq!(data, 0x88);
    }

    #[test]
    fn test_vram_horizontal_mirror() {
        // Horizontal: https://wiki.nesdev.com/w/index.php/Mirroring
        //   [0x2000 A] [0x2400 A]  A = 0x2000, B = 0x2400
        //   [0x2800 B] [0x2C00 B]

        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.write_to_port(0x2006, 0x24);
        ppu.write_to_port(0x2006, 0x05); // In horizontal mirroring, 0x2405 should be mirrored to 0x2005
        ppu.write_to_port(0x2007, 0x66); // Write 0x66 to 0x2005

        ppu.write_to_port(0x2006, 0x28); // In horizontal mirroring, 0x2805 should be mirrored to 0x2405
        ppu.write_to_port(0x2006, 0x05);
        ppu.write_to_port(0x2007, 0x77); // Write 0x77 to 0x2405

        ppu.write_to_port(0x2006, 0x20);
        ppu.write_to_port(0x2006, 0x05);
        let _ = ppu.read_port(0x2007); // Load value at 0x2005 into buffer
        let data = ppu.read_port(0x2007); // Get value in buffer
        assert_eq!(data, 0x66);

        ppu.write_to_port(0x2006, 0x2C);
        ppu.write_to_port(0x2006, 0x05); // In horizontal mirroring, 0x2C05 should be mirrored to 0x2405
        let _ = ppu.read_port(0x2007); // Load value at 0x2405 into buffer
        let data = ppu.read_port(0x2007); // Get value in buffer
        assert_eq!(data, 0x77);
    }

    #[test]
    fn test_vram_vertical_mirror() {
        // Vertical: https://wiki.nesdev.com/w/index.php/Mirroring
        //   [0x2000 A] [0x2400 B]  A = 0x2000, B = 0x2400
        //   [0x2800 A] [0x2C00 B]

        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Vertical);

        ppu.write_to_port(0x2006, 0x20);
        ppu.write_to_port(0x2006, 0x05);
        ppu.write_to_port(0x2007, 0x66); // Write 0x66 to 0x2005

        ppu.write_to_port(0x2006, 0x24);
        ppu.write_to_port(0x2006, 0x05);
        ppu.write_to_port(0x2007, 0x77); // Write 0x77 to 0x2405

        ppu.write_to_port(0x2006, 0x28);
        ppu.write_to_port(0x2006, 0x05);
        let _ = ppu.read_port(0x2007);
        let data = ppu.read_port(0x2007);
        assert_eq!(data, 0x66); // If 0x2005 == 0x2805

        ppu.write_to_port(0x2006, 0x2C);
        ppu.write_to_port(0x2006, 0x05);
        let _ = ppu.read_port(0x2007);
        let data = ppu.read_port(0x2007);
        assert_eq!(data, 0x77); // If 0x2405 == 0x2C05
    }

    #[test]
    fn test_read_status_resets_latch() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.vram[0x0305] = 0x66;

        ppu.write_to_port(0x2006, 0x21);

        ppu.read_port(0x2002); // Reset latch

        ppu.write_to_port(0x2006, 0x23);
        ppu.write_to_port(0x2006, 0x05);

        let _ = ppu.read_port(0x2007);
        let data = ppu.read_port(0x2007);
        assert_eq!(data, 0x66);
    }

    #[test]
    fn test_ppu_vram_mirroring() {
        let mut ppu = PPU::new(vec![0; 2048], Mirroring::Horizontal);
        ppu.write_to_port(0x2000, 0);
        ppu.vram[0x0305] = 0x66;

        ppu.write_to_port(0x2006, 0x63); //0x6305 -> 0x2305
        ppu.write_to_port(0x2006, 0x05);

        let _ = ppu.read_port(0x2007); // Load value at 0x2305 into buffer
        let data = ppu.read_port(0x2007); // Get value in buffer
        assert_eq!(data, 0x66);
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
