#[derive(Debug)]
pub struct Timer {
    pub counter: u16,
    pub period: u16,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            counter: 0,
            period: 0,
        }
    }

    pub fn tick(&mut self) -> bool {
        if self.counter == 0 {
            self.counter = self.period;
            true
        } else {
            self.counter -= 1;
            false
        }
    }

    pub fn update_period(&mut self, period: u16) {
        self.period = period;
        self.counter = period;
    }
}

#[cfg(test)]
mod tests {
    use super::Timer;

    #[test]
    fn test_tick() {
        let mut t = Timer::new();
        t.update_period(1);
        let r = t.tick();

        assert!(!r);

        let r = t.tick();

        assert!(r);
        assert_eq!(t.counter, 1);
    }
}
