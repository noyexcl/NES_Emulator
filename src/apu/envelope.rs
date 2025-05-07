/// 時間とともに変化するパラメータを再現する \
/// パラメータは15段階あり、一定期間が経過すると1ずつ減衰していく
#[derive(Debug)]
pub struct Envelope {
    counter: u8,
    pub period: u8,
    decay_level: u8,
    pub start_flag: bool,
    pub looping: bool,
    // Constant volume flag はボリュームの元を設定する他、何の効果も持たない。Decay level は Constant Volume が選択されている間も更新され続ける
    pub constant_volume: bool,
}

impl Envelope {
    pub fn new() -> Self {
        Self {
            counter: 0,
            period: 0,
            decay_level: 0,
            start_flag: false,
            looping: false,
            constant_volume: false,
        }
    }

    pub fn clock(&mut self) {
        // フレームカウンターによってクロックされた時、2つのアクションのうちいずれかが起こる
        // 1. Start flag がクリア
        //   - Divider がクロックされる
        // 2. Start flag がセット
        //   - Start flag はクリアされ、Decay level counter が15でロードされる。
        //   - そして Divider の期限が即座にリロードされる
        //
        // Divider が0でクロックされると対応するレジスターで設定された値Vでロードされ、Decay level counter をクロックする
        //
        // Decay level counter がクロックされると2つのアクションのうちいずれかが起こる
        // 1. カウンターが0ではない
        //   - デクリメントされる
        // 2. カウンターが0であり、ループフラグがセットされている
        //   - Decay level counter が15でロードされる

        if self.start_flag {
            self.start_flag = false;
            self.decay_level = 15;
            self.counter = self.period;
            return;
        }

        if self.counter == 0 {
            self.counter = self.period;

            if self.decay_level > 0 {
                self.decay_level -= 1;
            } else if self.looping {
                self.decay_level = 15;
            }
        } else {
            self.counter -= 1;
        }
    }

    /// 0~15 の範囲で現在のボリュームを返す \
    /// Constant volume がオンならば常に period の値を返す
    pub fn current_volume(&self) -> u8 {
        if self.constant_volume {
            self.period
        } else {
            self.decay_level
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Envelope;

    #[test]
    fn test_start_flag() {
        let mut envelope = Envelope::new();
        envelope.start_flag = true;
        envelope.period = 20;

        envelope.clock();

        assert!(!envelope.start_flag);
        assert_eq!(20, envelope.counter);
        assert_eq!(15, envelope.decay_level);

        envelope.clock();

        assert!(!envelope.start_flag);
        assert_eq!(19, envelope.counter);
        assert_eq!(15, envelope.decay_level);
    }

    #[test]
    fn test_clock_at_0() {
        let mut envelope = Envelope::new();
        envelope.decay_level = 15;
        envelope.clock();

        assert_eq!(14, envelope.decay_level);

        envelope.decay_level = 0;
        envelope.looping = true;
        envelope.clock();

        assert_eq!(15, envelope.decay_level);
    }

    #[test]
    fn test_volume() {
        let mut envelope = Envelope::new();
        envelope.constant_volume = true;
        envelope.period = 7;

        assert_eq!(7, envelope.current_volume());

        envelope.constant_volume = false;
        envelope.decay_level = 10;

        assert_eq!(10, envelope.current_volume());
    }
}
