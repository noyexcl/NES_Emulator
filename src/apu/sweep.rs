use super::timer::Timer;

#[derive(Debug)]
pub struct Sweep {
    pub mute: bool,
    pub enabled: bool,
    pub negate: bool,
    is_channel1: bool,
    pub reload: bool,
    pub shift: u8,
    counter: u8,
    pub period: u8,
}

impl Sweep {
    pub fn new(is_channel1: bool) -> Self {
        Self {
            mute: false,
            enabled: false,
            negate: false,
            is_channel1,
            reload: false,
            shift: 0,
            counter: 0,
            period: 0,
        }
    }

    pub fn clock(&mut self, timer: &mut Timer) {
        // 目標周期はずっと計算されるので、Sweep の加算器から生じた目標周期のオーバーフローは、Sweep ユニットが無効になっていたり、
        // Sweep divider がクロック信号を送っていなかったとしてもチャンネルをミュートにする
        let target = self.target_period(timer.period);
        self.mute = target > 0x7ff || timer.period < 8;

        if self.counter == 0 && self.enabled && self.shift > 0 && !self.mute {
            timer.update_period(target);
        }

        if self.counter == 0 || self.reload {
            self.counter = self.period;
            self.reload = false;
        } else {
            self.counter -= 1;
        }
    }

    fn target_period(&mut self, current_period: u16) -> u16 {
        let change_amount = current_period >> self.shift;

        if self.negate {
            current_period
                .wrapping_sub(change_amount)
                .wrapping_sub(self.is_channel1 as u16)
        } else {
            current_period + change_amount
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Sweep;
    use super::Timer;

    #[test]
    fn test_calc_target_period() {
        let mut s = Sweep::new(true);
        s.shift = 2;
        let target = s.target_period(10);

        assert_eq!(target, (10 >> 2) + 10);

        s.negate = true;
        let target = s.target_period(10);

        assert_eq!(target, 10 - (10 >> 2) - 1);

        s.is_channel1 = false;
        let target = s.target_period(10);

        assert_eq!(target, 10 - (10 >> 2));
    }

    #[test]
    fn test_clock() {
        let mut t = Timer::new();
        t.period = 10;

        let mut s = Sweep::new(true);
        s.counter = 0;
        s.enabled = true;
        s.shift = 1;

        s.clock(&mut t);

        assert_eq!(t.period, (10 >> 1) + 10);
    }

    #[test]
    fn test_period_too_small_mute() {
        let mut t = Timer::new();
        t.period = 7;

        let mut s = Sweep::new(true);
        s.period = 10;
        s.counter = 0;
        s.enabled = true;
        s.shift = 1;

        s.clock(&mut t);

        assert!(s.mute);
        assert_eq!(t.period, 7);
        assert_eq!(s.counter, 10);
    }

    #[test]
    fn test_overflow_mute() {
        let mut t = Timer::new();
        t.period = 0x7000;

        let mut s = Sweep::new(true);
        s.enabled = true;
        s.shift = 1;

        s.clock(&mut t);

        assert!(s.mute);
        assert_eq!(t.period, 0x7000);
    }

    #[test]
    fn test_reload() {
        let mut t = Timer::new();
        let mut s = Sweep::new(true);
        s.period = 10;
        s.counter = 0;
        s.clock(&mut t);

        assert_eq!(s.counter, 10);

        s.counter = 7;
        s.reload = true;
        s.clock(&mut t);

        assert!(!s.reload);
        assert_eq!(s.counter, 10);
    }
}
