#[derive(Debug)]
pub struct Sequencer {
    pub current_step: usize,
    steps: usize,
}

impl Sequencer {
    pub fn new(steps: usize) -> Self {
        Self {
            current_step: 0,
            steps,
        }
    }

    pub fn clock(&mut self) {
        self.current_step = (self.current_step + 1) % self.steps;
    }

    pub fn reset(&mut self) {
        self.current_step = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::Sequencer;

    #[test]
    fn test_clock() {
        let mut s = Sequencer::new(8);
        s.clock();

        assert_eq!(s.current_step, 1);

        s.current_step = 7;
        s.clock();

        assert_eq!(s.current_step, 0);
    }
}
