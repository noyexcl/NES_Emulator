use super::{envelope::Envelope, length_counter::LengthCounter, timer::Timer};

pub struct Noise {
    timer: Timer,
    envelope: Envelope,
    pub length_counter: LengthCounter,
    shift_reg: u16,
    mode: bool,
}

impl Noise {
    const TIMER_PERIOD: [u16; 16] = [
        4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
    ];

    pub fn new() -> Self {
        Self {
            timer: Timer::new(),
            envelope: Envelope::new(),
            length_counter: LengthCounter::new(),
            shift_reg: 1,
            mode: false,
        }
    }

    pub fn apu_tick(&mut self) {
        if self.timer.tick() {
            let bit1 = self.shift_reg & 1;
            let bit2 = (self.shift_reg >> (if self.mode { 6 } else { 1 })) & 1;

            let feedback = bit1 ^ bit2;

            self.shift_reg >>= 1;
            self.shift_reg |= feedback << 14;
        }
    }

    pub fn quarter_frame_clock(&mut self) {
        self.envelope.clock();
    }

    pub fn half_frame_clock(&mut self) {
        self.envelope.clock();
        self.length_counter.clock();
    }

    /// $400C (write) \
    /// --lc vvvv
    ///
    /// l: Length counter halt flag \
    /// c: volume/envelope flag \
    /// v: volume/envelope's divider period
    pub fn write_main_register(&mut self, val: u8) {
        self.length_counter.set_halted((val & 0b0010_0000) != 0);
        self.envelope.constant_volume = (val & 0b0001_0000) != 0;
        self.envelope.period = val & 0x0F;
    }

    /// $400E (write) \
    /// M--- PPPP
    ///
    /// M: Mode flag \
    /// P: Timer period(index of table)
    pub fn write_mode_period_register(&mut self, val: u8) {
        self.mode = (val & 0b1000_0000) != 0;
        self.timer.period = Self::TIMER_PERIOD[(val & 0xF) as usize];
    }

    /// $400F (write) \
    /// llll l---
    ///
    /// l: length counter reload
    ///
    /// Side effects: set envelope start flag
    pub fn write_length_register(&mut self, val: u8) {
        self.length_counter.set_length(val >> 3);

        // Side effect
        self.envelope.start_flag = true;
    }

    pub fn sample(&self) -> u8 {
        if self.shift_reg & 1 == 0 && self.length_counter.is_active() {
            self.envelope.current_volume()
        } else {
            0
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.length_counter.set_enabled(enabled);
    }

    pub fn is_playing(&self) -> bool {
        self.length_counter.is_active()
    }
}
