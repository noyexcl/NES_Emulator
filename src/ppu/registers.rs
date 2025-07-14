use bitflags::bitflags;

bitflags! {
    // 7  bit  0
    // ---- ----
    // VPHB SINN
    // |||| ||||
    // |||| ||++- Base nametable address
    // |||| ||    (0 = $2000; 1 = $2400; 2 = $2800; 3 = $2C00)
    // |||| |+--- VRAM address increment per CPU read/write of PPUDATA
    // |||| |     (0: add 1, going across; 1: add 32, going down)
    // |||| +---- Sprite pattern table address for 8x8 sprites
    // ||||       (0: $0000; 1: $1000; ignored in 8x16 mode)
    // |||+------ Background pattern table address (0: $0000; 1: $1000)
    // ||+------- Sprite size (0: 8x8 pixels; 1: 8x16 pixels)
    // |+-------- PPU master/slave select
    // |          (0: read backdrop from EXT pins; 1: output color on EXT pins)
    // +--------- Generate an NMI at the start of the
    //            vertical blanking interval (0: off; 1: on)
    #[derive(Debug)]
    pub struct PPUCTRL: u8 {
        const NAMETABLE1              = 0b0000_0001;
        const NAMETABLE2              = 0b0000_0010;
        const VRAM_ADD_INCREMENT      = 0b0000_0100;
        const SPRITE_PATTERN_ADDR     = 0b0000_1000;
        const BACKGROUND_PATTERN_ADDR = 0b0001_0000;
        const SPRITE_SIZE             = 0b0010_0000;
        const MASTER_SLAVE_SELECT     = 0b0100_0000;
        const GENERATE_NMI            = 0b1000_0000;
    }
}

impl PPUCTRL {
    pub fn new() -> Self {
        PPUCTRL::from_bits_truncate(0b0000_0000)
    }

    pub fn vram_addr_increment(&self) -> u8 {
        if self.contains(PPUCTRL::VRAM_ADD_INCREMENT) {
            32
        } else {
            1
        }
    }

    pub fn sprite_pattern_addr(&self) -> u16 {
        if self.contains(PPUCTRL::SPRITE_PATTERN_ADDR) {
            0x1000
        } else {
            0
        }
    }

    pub fn background_pattern_addr(&self) -> u16 {
        if self.contains(PPUCTRL::BACKGROUND_PATTERN_ADDR) {
            0x1000
        } else {
            0
        }
    }

    pub fn sprite_size(&self) -> u16 {
        if self.contains(PPUCTRL::SPRITE_SIZE) {
            16
        } else {
            8
        }
    }

    pub fn master_select(&self) -> u8 {
        if self.contains(PPUCTRL::MASTER_SLAVE_SELECT) {
            1
        } else {
            0
        }
    }

    pub fn generate_vblank_nmi(&self) -> bool {
        self.contains(PPUCTRL::GENERATE_NMI)
    }
}

bitflags! {
    // 7  bit  0
    // ---- ----
    // BGRs bMmG
    // |||| ||||
    // |||| |||+- Greyscale (0: normal color, 1: produce a greyscale display)
    // |||| ||+-- 1: Show background in leftmost 8 pixels of screen, 0: Hide
    // |||| |+--- 1: Show sprites in leftmost 8 pixels of screen, 0: Hide
    // |||| +---- 1: Show background
    // |||+------ 1: Show sprites
    // ||+------- Emphasize red (green on PAL/Dendy)
    // |+-------- Emphasize green (red on PAL/Dendy)
    // +--------- Emphasize blue
    #[derive(Debug)]
    pub struct PPUMASK: u8 {
        const GREYSCALE        = 0b0000_0001;
        const LEFTMOST_SP     = 0b0000_0010;
        const LEFTMOST_BG      = 0b0000_0100;
        const SHOW_BACKGROUND  = 0b0000_1000;
        const SHOW_SPRITES     = 0b0001_0000;
        const EMPHASIZE_RED    = 0b0010_0000;
        const EMPHASIZE_GREEN  = 0b0100_0000;
        const EMPHASIZE_BLUE   = 0b1000_0000;
    }
}

impl Default for PPUMASK {
    fn default() -> Self {
        PPUMASK::from_bits_truncate(0b0000_0000)
    }
}

bitflags! {
    // 7  bit  0
    // ---- ----
    // VSO. ....
    // |||| ||||
    // |||+-++++- PPU open bus. Returns stale PPU bus contents.
    // ||+------- Sprite overflow. The intent was for this flag to be set
    // ||         whenever more than eight sprites appear on a scanline, but a
    // ||         hardware bug causes the actual behavior to be more complicated
    // ||         and generate false positives as well as false negatives; see
    // ||         PPU sprite evaluation. This flag is set during sprite
    // ||         evaluation and cleared at dot 1 (the second dot) of the
    // ||         pre-render line.
    // |+-------- Sprite 0 Hit.  Set when a nonzero pixel of sprite 0 overlaps
    // |          a nonzero background pixel; cleared at dot 1 of the pre-render
    // |          line.  Used for raster timing.
    // +--------- Vertical blank has started (0: not in vblank; 1: in vblank).
    //            Set at dot 1 of line 241 (the line *after* the post-render
    //            line); cleared after reading $2002 and at dot 1 of the
    //            pre-render line.
    #[derive(Debug)]
    pub struct PPUSTATUS: u8 {
        const NOTUSED         = 0b0000_0001;
        const NOTUSED2        = 0b0000_0010;
        const NOTUSED3        = 0b0000_0100;
        const NOTUSED4        = 0b0000_1000;
        const NOTUSED5        = 0b0001_0000;
        const SPRITE_OVERFLOW = 0b0010_0000;
        const SPRITE_ZERO_HIT = 0b0100_0000;
        const VBLANK_STARTED  = 0b1000_0000;
    }
}

impl PPUSTATUS {
    pub fn new() -> Self {
        PPUSTATUS::from_bits_truncate(0b0000_0000)
    }

    pub fn set_vblank_status(&mut self, status: bool) {
        self.set(PPUSTATUS::VBLANK_STARTED, status);
    }

    pub fn set_sprite_zero_hit(&mut self, status: bool) {
        self.set(PPUSTATUS::SPRITE_ZERO_HIT, status);
    }

    pub fn set_sprite_overflow(&mut self, status: bool) {
        self.set(PPUSTATUS::SPRITE_OVERFLOW, status);
    }

    pub fn reset_vblank_status(&mut self) {
        self.remove(PPUSTATUS::VBLANK_STARTED);
    }

    pub fn is_in_vblank(&self) -> bool {
        self.contains(PPUSTATUS::VBLANK_STARTED)
    }
}
