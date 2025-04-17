use super::{
    length_counter::LengthCounter, linear_counter::LinearCounter, sequencer::Sequencer,
    timer::Timer,
};

const TRIANGLE_WAVEFORM: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
    13, 14, 15,
];

//       Linear Counter   Length Counter
//             |                |
//             v                v
// Timer ---> Gate ----------> Gate ---> Sequencer ---> (to mixer)
//
pub struct Triangle {
    timer: Timer,
    length_counter: LengthCounter,
    linear_counter: LinearCounter,
    sequencer: Sequencer,
}

impl Triangle {
    pub fn new() -> Self {
        Self {
            timer: Timer::new(),
            length_counter: LengthCounter::new(),
            linear_counter: LinearCounter::new(),
            sequencer: Sequencer::new(32),
        }
    }

    pub fn clock(&mut self) {
        if self.timer.tick()
            && self.linear_counter.is_active()
            && self.length_counter.is_active()
            && self.timer.period >= 2
        {
            self.sequencer.clock();
        }
    }

    pub fn clock_quarter_frame(&mut self) {
        self.linear_counter.clock();
    }

    pub fn clock_half_frame(&mut self) {
        self.linear_counter.clock();
        self.length_counter.clock();
    }

    pub fn sample(&self) -> u8 {
        TRIANGLE_WAVEFORM[self.sequencer.current_step]
    }

    /// $4008 Linear counter setup (write)
    /// * val - CRRR RRRR
    ///
    /// C: Control flag & length counter halt flag \
    /// R: Counter reload value
    pub fn write_linear_counter_setup(&mut self, val: u8) {
        let ctrl_and_halt = (0b1000_0000 & val) != 0;
        self.linear_counter.ctrl = ctrl_and_halt;
        self.length_counter.halted = ctrl_and_halt;
        self.linear_counter.period = val >> 1;
    }

    /// $400A Timer low (write)
    /// * val - LLLL LLLL
    ///
    /// L: Timer low 8 bits
    pub fn write_timer_lo(&mut self, val: u8) {
        self.timer.period = (self.timer.period & 0xFF00) | val as u16;
    }

    /// $400B Length counter load and timer high (write)
    /// * val - llll lHHH
    ///
    /// l: Length counter load value \
    /// H: Timer high 3 bits
    pub fn write_length_and_timer_hi(&mut self, val: u8) {
        self.timer.period = (self.timer.period & 0x00FF) | (val as u16) << 8;
        self.length_counter.set_length(val >> 3);

        // Side effects
        self.linear_counter.reload = true;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.length_counter.set_enabled(enabled);
    }

    /// Whether its length counter is not zero
    pub fn is_playing(&self) -> bool {
        self.length_counter.is_active()
    }
}
