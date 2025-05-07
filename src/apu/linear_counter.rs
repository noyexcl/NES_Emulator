#[derive(Debug)]
pub struct LinearCounter {
    counter: u8,
    pub period: u8,
    pub reload: bool,
    pub ctrl: bool,
}

impl LinearCounter {
    pub fn new() -> Self {
        Self {
            counter: 0,
            period: 0,
            reload: false,
            ctrl: false,
        }
    }

    pub fn clock(&mut self) {
        // フレームカウンタがリニアカウンタをクロックする時、以下のアクションが順に起きる
        // 1. もしリニアカウンタのリロードフラグがセットされていたら、リニアカウンタは値をリロードする。そうでなく、カウンタが0でなければデクリメントされる
        // 2. コントロールフラグがクリアされていたら、リニアカウンタのリロードフラグがクリアされる
        if self.reload {
            self.counter = self.period;
        } else if self.counter > 0 {
            self.counter -= 1;
        }

        // コントロールフラグがクリアされない限り、リロードフラグはクリアされない
        if !self.ctrl {
            self.reload = false;
        }
    }

    pub fn is_active(&self) -> bool {
        self.counter > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock() {
        let mut l = LinearCounter::new();

        l.period = 10;
        l.reload = true;
        l.ctrl = true;

        l.clock();

        assert!(l.reload);
        assert_eq!(l.counter, 10);

        l.ctrl = false;
        l.clock();

        assert!(!l.reload);
        assert_eq!(l.counter, 10);

        l.clock();

        assert_eq!(l.counter, 9);
    }
}
