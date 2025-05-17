#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Mode {
    Step4,
    Step5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    None,
    Quarter,
    Half,
}

#[derive(Debug)]
pub struct FrameCounter {
    mode: Mode,
    counter: u32,
    step: u8,
    pub irq_flag: bool,
    irq_enabled: bool,
    reset: Option<u8>,
}

impl FrameCounter {
    const STEP4_CYCLES: [u32; 6] = [7_457, 14_913, 22_371, 29_828, 29_829, 29_830];
    const STEP5_CYCLES: [u32; 6] = [7_457, 14_913, 22_371, 29_829, 37_281, 37_282];
    const FRAME_STEPS: [FrameType; 6] = [
        FrameType::Quarter,
        FrameType::Half,
        FrameType::Quarter,
        FrameType::None,
        FrameType::Half,
        FrameType::None,
    ];

    pub fn new() -> Self {
        Self {
            mode: Mode::Step4,
            counter: 0,
            step: 0,
            irq_flag: false,
            irq_enabled: true,
            reset: None,
        }
    }

    /// MI-- ---- \
    /// M : Sequencer mode: 0 selects 4-step sequence, 1 selects 5-step sequence \
    /// I : Interrupt inhibit flag. If set, the frame interrupt flag is cleared, otherwise it is unaffected. \
    ///
    /// Side Effects: Timer will reset after 3 or 4 CPU cycles \
    ///               Writing a value with bit 7 set to this register should generate
    ///               both of quater-frame and half-frame signals.
    pub fn write_register(&mut self, val: u8) {
        if val & 0b1000_0000 != 0 {
            self.mode = Mode::Step5;
        } else {
            self.mode = Mode::Step4;
        }

        if val & 0b0100_0000 != 0 {
            self.irq_enabled = false;
            self.irq_flag = false;
        } else {
            self.irq_enabled = true;
        }

        // If the write occurs during an APU cycle, in other words, at 1 CPU cycle(0.5 APU cycle)
        // then the timer will get reset after 3 cpu cycles delay.
        // if not, it will get reset after 4 CPU cycles delay.
        if self.counter & 1 == 1 {
            self.reset = Some(3);
        } else {
            self.reset = Some(4);
        }
    }

    pub fn tick(&mut self) -> FrameType {
        // $4017への書き込みから、タイマーがリセットされるまで 3,4 CPU cycle 遅延があるわけだが、この間のクロックはどう処理すべきなんだろう？
        // この間に進めてあるステップに到達したらどうなるのか？
        // とりあえず遅延待ちの時は、通常のクロックは停止した状態で実装してみる
        if let Some(delay) = self.reset {
            if delay == 1 {
                self.counter = 0;
                self.step = 0;
                self.reset = None;
            } else {
                self.reset = Some(delay - 1);
            }

            return FrameType::None;
        }

        self.counter += 1;

        let cycles_to_frame = match self.mode {
            Mode::Step4 => Self::STEP4_CYCLES[self.step as usize],
            Mode::Step5 => Self::STEP5_CYCLES[self.step as usize],
        };

        if self.counter == cycles_to_frame {
            let frame_type = Self::FRAME_STEPS[self.step as usize];

            if self.mode == Mode::Step4 && self.step >= 3 {
                self.trigger_irq();
            }

            self.step += 1;

            if self.step > 5 {
                self.counter = 0;
                self.step = 0;
            }

            frame_type
        } else {
            FrameType::None
        }
    }

    fn trigger_irq(&mut self) {
        if self.irq_enabled {
            self.irq_flag = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::apu::frame_counter::FrameType;

    use super::{FrameCounter, Mode};

    #[test]
    fn test_write_register() {
        let mut frame_counter = FrameCounter::new();
        frame_counter.counter = 1;
        frame_counter.write_register(0b1100_0000);

        assert_eq!(frame_counter.mode, Mode::Step5);
        assert!(!frame_counter.irq_enabled);
        assert_eq!(frame_counter.reset, Some(3));

        frame_counter.counter = 2;
        frame_counter.write_register(0b0000_0000);

        assert_eq!(frame_counter.mode, Mode::Step4);
        assert!(frame_counter.irq_enabled);
        assert_eq!(frame_counter.reset, Some(4));
    }

    #[test]
    fn test_tick_step4() {
        let mut frame_counter = FrameCounter::new();
        frame_counter.mode = Mode::Step4;
        frame_counter.irq_enabled = true;

        frame_counter.counter = 7456;
        let frame = frame_counter.tick();

        assert_eq!(frame, FrameType::Quarter);

        frame_counter.counter = 14912;
        let frame = frame_counter.tick();

        assert_eq!(frame, FrameType::Half);

        frame_counter.counter = 22370;
        let frame = frame_counter.tick();

        assert_eq!(frame, FrameType::Quarter);

        frame_counter.counter = 29827;
        let frame = frame_counter.tick();

        assert_eq!(frame, FrameType::None);
        assert!(frame_counter.irq_flag);

        frame_counter.counter = 29828;
        let frame = frame_counter.tick();

        assert_eq!(frame, FrameType::Half);

        frame_counter.counter = 29829;
        let frame = frame_counter.tick();

        assert_eq!(frame, FrameType::None);
    }

    #[test]
    fn test_tick_step5() {
        let mut frame_counter = FrameCounter::new();
        frame_counter.mode = Mode::Step5;
        frame_counter.irq_enabled = true;

        frame_counter.counter = 7456;
        let frame = frame_counter.tick();

        assert_eq!(frame, FrameType::Quarter);

        frame_counter.counter = 14912;
        let frame = frame_counter.tick();

        assert_eq!(frame, FrameType::Half);

        frame_counter.counter = 22370;
        let frame = frame_counter.tick();

        assert_eq!(frame, FrameType::Quarter);

        frame_counter.counter = 29828;
        let frame = frame_counter.tick();

        assert_eq!(frame, FrameType::None);
        assert!(!frame_counter.irq_flag);

        frame_counter.counter = 37280;
        let frame = frame_counter.tick();

        assert_eq!(frame, FrameType::Half);
        assert!(!frame_counter.irq_flag);

        frame_counter.counter = 37281;
        let frame = frame_counter.tick();

        assert_eq!(frame, FrameType::None);
        assert!(!frame_counter.irq_flag);
    }

    #[test]
    fn test_reset() {
        let mut f = FrameCounter::new();
        f.counter = 10;
        f.write_register(0b1000_0000);

        assert_eq!(f.reset, Some(4));

        for _ in 0..3 {
            f.tick();
        }

        let frame = f.tick();

        assert_eq!(f.reset, None);
        assert_eq!(f.counter, 0);
        assert_eq!(frame, FrameType::None);
    }
}
