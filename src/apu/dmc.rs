use std::rc::Rc;

use crate::rom::Rom;

use super::timer::Timer;

//                         Timer
//                           |
//                           v
// Reader ---> Buffer ---> Shifter ---> Output level ---> (to the mixer)
//
// 1. サンプルが格納されているアドレスとその長さが設定される
// 2. メモリリーダが1バイトずつバッファに読み込む
// 3. タイマーによって励起された Shifter により、1ビットずつ処理される。この処理により Output level の値が変化する
//    処理が終わるとバッファを空にする(そしてメモリリーダが次のバイトを読み込む)
// 4. Output level は起動時に0でロードされ、Shifter により更新される
//    それ以外にも$4011への書き込みで直接値を設定することも出来る
//    Output level は 7bit の値であり、チャンネルが有効か無効かに関わらずミキサに送られる
#[allow(clippy::upper_case_acronyms)]
pub struct DMC {
    timer: Timer,
    sample_buffer: u8,
    sample_addr: u16,
    sample_length: u16,
    current_addr: u16,
    current_length: u16,
    shift_register: u8,
    remaining_bits: u8,
    output_level: u8,
    looping: bool,
    silence: bool,
    irq_enabled: bool,
    pub irq_flag: bool,
    enabled: bool,
    rom: Rc<Rom>,
    pub cpu_stall: usize,
}

impl DMC {
    const RATE_TABLE: [u16; 16] = [
        428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
    ];

    pub fn new(rom: Rc<Rom>) -> Self {
        Self {
            timer: Timer::new(),
            sample_buffer: 0,
            sample_addr: 0,
            sample_length: 0,
            current_addr: 0,
            current_length: 0,
            shift_register: 0,
            remaining_bits: 0,
            output_level: 0,
            looping: false,
            silence: true,
            irq_enabled: false,
            irq_flag: false,
            enabled: false,
            rom,
            cpu_stall: 0,
        }
    }

    pub fn apu_tick(&mut self) {
        if !self.enabled {
            return;
        }

        // If there are remaining bits, these will be played first
        if self.sample_buffer == 0 && self.current_length > 0 {
            // Loading sample causes CPU stall (by 1~4 cycle)
            // The exact cycle depends on many factors but here we just set it to 4
            self.cpu_stall += 4;

            self.load_sample();
        }

        if self.timer.tick() {
            self.clock_shifter();
        }
    }

    /// $4010 Flags and Rate (write) \
    /// IL-- RRRR
    ///
    /// I: IRQ enabled flag. if clear, interrupt flag is cleard \
    /// L: Loop flag \
    /// R: Rate index \
    ///
    /// The rate determines for how many CPU cycles happen between changes in the output level \
    /// during automatic delta-encoded sample playback (DMC's timer period)
    pub fn write_flags_rate(&mut self, val: u8) {
        self.irq_enabled = (val & 0b1000_0000) != 0;

        if !self.irq_enabled {
            self.irq_flag = false;
        }

        self.looping = (val & 0b0100_0000) != 0;

        self.timer.period = Self::RATE_TABLE[(val & 0b0000_1111) as usize] / 2;
    }

    /// $4011 Direct load (write) \
    /// -DDD DDDD
    ///
    /// D: The DMC output level is set to D
    pub fn write_direct_load(&mut self, val: u8) {
        self.output_level = val & 0b0111_1111;
    }

    /// $4012 Sample address (write) \
    /// AAAA AAAA
    ///
    /// A: Sample address = $C000 + (A * 64)
    pub fn write_sample_address(&mut self, val: u8) {
        self.sample_addr = (val as u16) * 64 + 0xC000;

        println!("Sample address: {:#X}", self.sample_addr);
    }

    /// $4013 Sample length (write) \
    /// LLLL LLLL
    ///
    /// L: Sample length = (L * 16) + 1 byte
    pub fn write_sample_length(&mut self, val: u8) {
        self.sample_length = val as u16 * 16 + 1;
    }

    pub fn sample(&mut self) -> u8 {
        // The output level is sent to the mixer whether the channel is enabled or not
        self.output_level
    }

    /// Enable/Disable this channel (called through $4015 register)
    ///
    /// When `enabled`, *only* if the current length is 0, it restarts the sample. \
    /// (otherwise, it will just resume playing?) \
    ///
    /// If there are remaining bits that not played yet, these will be played before the next sample fetched.
    ///
    /// When `disabled`, the current length is set to 0 and DMC will silence when it empties. \
    ///
    /// DMC interrupt flag is cleared as a side effect of writing to $4015 register.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.irq_flag = false;

        if !enabled {
            self.current_length = 0;
        } else if self.current_length == 0 {
            self.current_addr = self.sample_addr;
            self.current_length = self.sample_length;
        }
    }

    fn load_sample(&mut self) {
        self.sample_buffer = self.rom.read_prg_rom(self.current_addr);

        if self.current_addr == 0xFFFF {
            self.current_addr = 0x8000;
        } else {
            self.current_addr += 1;
        }

        self.current_length -= 1;

        if self.current_length == 0 {
            if self.looping {
                self.current_addr = self.sample_addr;
                self.current_length = self.sample_length;
            } else if self.irq_enabled {
                self.irq_flag = true;
            }
        }
    }

    /// Corresponds to bit 4 of $4015 (read) register.
    ///
    /// Return true if DMC remaining bytes is more than 0.
    pub fn is_playing(&self) -> bool {
        self.current_length > 0
    }

    fn clock_shifter(&mut self) {
        if !self.silence {
            if self.shift_register & 1 == 1 && self.output_level <= 125 {
                self.output_level += 2;
            } else if self.shift_register & 1 == 0 && self.output_level >= 2 {
                self.output_level -= 2;
            }
        }

        self.shift_register >>= 1;

        if self.remaining_bits > 0 {
            self.remaining_bits -= 1;
        }

        if self.remaining_bits == 0 {
            self.remaining_bits = 8;

            if self.sample_buffer == 0 {
                self.silence = true;
            } else {
                self.silence = false;
                self.shift_register = self.sample_buffer;
                self.sample_buffer = 0;
            }
        }
    }
}
