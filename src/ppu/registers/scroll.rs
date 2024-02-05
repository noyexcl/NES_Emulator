pub struct ScrollRegister {
    x: u8,
    y: u8,
    latch: bool,
}

impl ScrollRegister {
    pub fn new() -> Self {
        ScrollRegister {
            x: 0,
            y: 0,
            latch: true,
        }
    }

    pub fn write(&mut self, data: u8) {
        // X first
        if self.latch {
            self.x = data;
        } else {
            self.y = data;
        }

        self.latch = !self.latch;
    }

    pub fn reset_latch(&mut self) {
        self.latch = true;
    }
}
