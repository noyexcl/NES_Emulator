use super::{
    envelope::Envelope, length_counter::LengthCounter, sequencer::Sequencer, sweep::Sweep,
    timer::Timer,
};

const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0],
    [0, 1, 1, 0, 0, 0, 0, 0],
    [0, 1, 1, 1, 0, 0, 0, 0],
    [1, 0, 0, 1, 1, 1, 1, 1],
];

//               Sweep -----> Timer
//                |            |
//                |            |
//                |            v
//                |        Sequencer   Length Counter
//                |            |             |
//                |            |             |
//                v            v             v
// Envelope ---> Gate -----> Gate -------> Gate --->(to mixer)
pub struct Pulse {
    envelop: Envelope,
    sweep: Sweep,
    timer: Timer,
    sequencer: Sequencer,
    length_counter: LengthCounter,
    duty: u8,
}

impl Pulse {
    pub fn new(is_channel1: bool) -> Self {
        Self {
            envelop: Envelope::new(),
            sweep: Sweep::new(is_channel1),
            timer: Timer::new(),
            sequencer: Sequencer::new(8),
            length_counter: LengthCounter::new(),
            duty: 0,
        }
    }

    pub fn tick_timer(&mut self) {
        if self.timer.tick() {
            self.sequencer.clock();
        }
    }

    pub fn clock_quarter_frame(&mut self) {
        self.envelop.clock();
    }

    pub fn clock_half_frame(&mut self) {
        self.clock_quarter_frame();
        self.length_counter.clock();
        self.sweep.clock(&mut self.timer);
    }

    pub fn sample(&self) -> u8 {
        if !self.sweep.mute && self.length_counter.is_active() {
            DUTY_TABLE[self.duty as usize][self.sequencer.current_step]
                * self.envelop.current_volume()
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

    pub fn write_main_register(&mut self, value: u8) {
        // Pulse Main Register (DDLC VVVV)
        // DD: Duty Cycle
        // L : Loop. If set, its counter will not decrease,
        //     Resulting in a tone that plays continuously.
        // C : Const Volume. If set, the sweep will not change its volume over time.
        // VVVV : Volume(C=1) or Envelope(C=0).

        // Changing duty doesn't affect current sequencer step
        self.duty = value >> 6;

        let halt_and_loop = (value & 0b0010_0000) != 0;
        self.length_counter.halted = halt_and_loop;
        self.envelop.looping = halt_and_loop;

        self.envelop.constant_volume = (value & 0b0001_0000) != 0;
        self.envelop.period = value & 0b0000_1111;
    }

    pub fn write_sweep_register(&mut self, value: u8) {
        // Sweep Register (EPPP NSSS)
        // E: Enable
        // P: Period
        // N: Negate
        // S: Shift
        self.sweep.enabled = (value & 0b1000_0000) != 0;
        self.sweep.period = (value >> 4) & 0b0000_0111;
        self.sweep.negate = (value & 0b0000_1000) != 0;
        self.sweep.shift = value & 0b0000_0111;

        // Side effect of writing to sweep register
        self.sweep.reload = true;
    }

    pub fn write_timer_lo(&mut self, value: u8) {
        self.timer.period = (self.timer.period & 0xFF00) | value as u16;
    }

    pub fn write_timer_hi_and_length(&mut self, value: u8) {
        // Length & Timer upper bits (LLLL LTTT)
        self.timer.period = (self.timer.period & 0x00FF) | ((value as u16 & 0b0000_0111) << 8);
        self.length_counter.set_length(value >> 3);

        // Side effect of writing to timer & length register
        self.sequencer.current_step = 0;
        self.envelop.start_flag = true;
    }
}

#[cfg(test)]
mod tests {
    use super::Pulse;

    #[test]
    fn test_write_main_register() {
        let mut p = Pulse::new(true);
        p.write_main_register(0b1110_1000);

        assert_eq!(p.duty, 0b11);
        assert!(p.envelop.looping);
        assert!(p.length_counter.halted);
        assert!(!p.envelop.constant_volume);
        assert_eq!(p.envelop.period, 0b1000);
    }

    #[test]
    fn test_write_sweep_register() {
        let mut p = Pulse::new(true);
        p.write_sweep_register(0b1100_1001);

        assert!(p.sweep.enabled);
        assert_eq!(p.sweep.period, 0b100);
        assert!(p.sweep.negate);
        assert_eq!(p.sweep.shift, 0b001);
    }

    #[test]
    fn test_write_timer_and_length() {
        let mut p = Pulse::new(true);
        p.write_timer_lo(0b1010_1010);
        p.write_timer_hi_and_length(0b0000_0111);

        assert_eq!(p.timer.period, 0b111_1010_1010);
        assert!(p.length_counter.is_active());
    }
}
