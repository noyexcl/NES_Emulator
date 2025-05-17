// see for details https://www.nesdev.org/wiki/APU_Length_Counter
const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

#[derive(Debug)]
pub struct LengthCounter {
    enabled: bool,
    pub halted: bool,
    counter: u8,
    new_halted: Option<bool>,
    new_counter: Option<u8>,
    prev_counter: u8,
}

impl LengthCounter {
    pub fn new() -> Self {
        Self {
            enabled: false,
            halted: false,
            counter: 0,
            new_halted: None,
            new_counter: None,
            prev_counter: 0,
        }
    }

    pub fn clock(&mut self) {
        if !self.halted && self.counter > 0 {
            self.counter -= 1;
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;

        if !enabled {
            self.counter = 0;
        }
    }

    /// New length will be loaded after a 1 CPU cycle delay. \
    /// If the counter is clocked during that cycle, new length will be discarded.
    pub fn set_length(&mut self, length: u8) {
        if self.enabled {
            self.new_counter = Some(LENGTH_TABLE[length as usize]);
            self.prev_counter = self.counter;
        }
    }

    pub fn set_halted(&mut self, halted: bool) {
        self.new_halted = Some(halted);
    }

    pub fn reload(&mut self) {
        if let Some(c) = self.new_counter {
            if self.counter == self.prev_counter {
                self.counter = c;
            }
            self.new_counter = None
        }

        self.halted = self.new_halted.take().unwrap_or(self.halted);
    }

    pub fn is_active(&self) -> bool {
        self.counter > 0
    }
}

#[cfg(test)]
mod tests {
    use super::LengthCounter;

    #[test]
    fn test_clock() {
        let mut l = LengthCounter::new();
        l.enabled = true;
        l.counter = 10;
        l.clock();

        assert_eq!(l.counter, 9);

        l.halted = true;
        l.clock();

        assert_eq!(l.counter, 9);

        l.set_enabled(false);

        assert_eq!(l.counter, 0);
    }

    #[test]
    fn test_set_length() {
        let mut l = LengthCounter::new();
        l.set_enabled(true);
        l.set_length(0x1f);
        l.reload();

        assert_eq!(l.counter, 30);

        l.set_length(0x01);
        l.reload();

        assert_eq!(l.counter, 254);
    }

    #[test]
    fn test_is_playing() {
        let mut l = LengthCounter::new();
        l.counter = 1;
        l.enabled = true;
        l.clock();

        assert!(!l.is_active());
    }
}
