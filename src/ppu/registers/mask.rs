use bitflags::bitflags;

pub enum Color {
    Red,
    Green,
    Blue,
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
    pub struct MaskRegister: u8 {
        const GREYSCALE        = 0b0000_0001;
        const LEFTMOST_OBJ     = 0b0000_0010;
        const LEFTMOST_BG      = 0b0000_0100;
        const SHOW_BACKGROUND  = 0b0000_1000;
        const SHOW_SPRITES     = 0b0001_0000;
        const EMPHASIZE_RED    = 0b0010_0000;
        const EMPHASIZE_GREEN  = 0b0100_0000;
        const EMPHASIZE_BLUE   = 0b1000_0000;
    }
}

impl MaskRegister {
    pub fn new() -> Self {
        MaskRegister::from_bits_truncate(0b0000_0000)
    }

    pub fn greyscale(&self) -> bool {
        self.contains(MaskRegister::GREYSCALE)
    }

    pub fn leftmost_8pxl_background(&self) -> bool {
        self.contains(MaskRegister::LEFTMOST_BG)
    }

    pub fn leftmost_8pxl_sprite(&self) -> bool {
        self.contains(MaskRegister::LEFTMOST_OBJ)
    }

    pub fn show_background(&self) -> bool {
        self.contains(MaskRegister::SHOW_BACKGROUND)
    }

    pub fn show_sprite(&self) -> bool {
        self.contains(MaskRegister::SHOW_SPRITES)
    }

    pub fn emphasise(&self) -> Vec<Color> {
        let mut result = vec![];
        if self.contains(MaskRegister::EMPHASIZE_RED) {
            result.push(Color::Red);
        }
        if self.contains(MaskRegister::EMPHASIZE_BLUE) {
            result.push(Color::Blue);
        }
        if self.contains(MaskRegister::EMPHASIZE_GREEN) {
            result.push(Color::Green);
        }
        result
    }
}
