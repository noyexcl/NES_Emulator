mod dmc;
mod envelope;
mod filter;
mod frame_counter;
mod length_counter;
mod linear_counter;
mod noise;
mod pulse;
mod sequencer;
mod sweep;
mod timer;
mod triangle;

use std::rc::Rc;

use dmc::DMC;
use filter::FirstOrderFilter;
use frame_counter::{FrameCounter, FrameType};
use noise::Noise;
use pulse::Pulse;
use triangle::Triangle;

use crate::rom::Rom;

#[allow(clippy::upper_case_acronyms)]
pub struct APU {
    buffer: Vec<i16>,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    dmc: DMC,
    frame_counter: FrameCounter,
    filters: [FirstOrderFilter; 3],
    cycles: usize,
}

impl APU {
    pub fn new(rom: Rc<Rom>) -> Self {
        Self {
            buffer: vec![],
            pulse1: Pulse::new(true),
            pulse2: Pulse::new(false),
            triangle: Triangle::new(),
            noise: Noise::new(),
            dmc: DMC::new(rom),
            frame_counter: FrameCounter::new(),
            filters: [
                FirstOrderFilter::high_pass(44100.0, 90.0),
                FirstOrderFilter::high_pass(44100.0, 440.0),
                FirstOrderFilter::low_pass(44100.0, 14_000.0),
            ],
            cycles: 0,
        }
    }

    pub fn tick(&mut self) {
        self.cycles += 1;

        self.triangle.clock();

        if self.cycles % 2 == 1 {
            self.pulse1.tick_timer();
            self.pulse2.tick_timer();
            self.noise.apu_tick();
            self.dmc.apu_tick();
        }

        match self.frame_counter.tick() {
            FrameType::Quarter => {
                self.clock_quarter_frame();
            }
            FrameType::Half => {
                self.clock_half_frame();
            }
            FrameType::None => (),
        }

        // We need 730 stereo audio samples per frame for 60 fps.
        // Each frame lasts a minimum of 29,779 CPU cycles. This
        // works out to around 40 CPU cycles per sample.
        if self.cycles % 40 == 0 {
            let s = self.sample();
            self.buffer.push(s);
            self.buffer.push(s);
        }
    }

    fn clock_quarter_frame(&mut self) {
        self.pulse1.clock_quarter_frame();
        self.pulse2.clock_quarter_frame();
        self.triangle.clock_quarter_frame();
        self.noise.quarter_frame_clock();
    }

    fn clock_half_frame(&mut self) {
        self.pulse1.clock_half_frame();
        self.pulse2.clock_half_frame();
        self.triangle.clock_half_frame();
        self.noise.half_frame_clock();
    }

    pub fn sample(&mut self) -> i16 {
        let pulse1 = self.pulse1.sample() as f64;
        let pulse2 = self.pulse2.sample() as f64;
        let t = self.triangle.sample() as f64;
        let n = self.noise.sample() as f64;
        let d = self.dmc.sample() as f64;

        let pulse_out = 95.88 / ((8218.0 / (pulse1 + pulse2)) + 100.0);
        let tnd_out = 159.79 / (1.0 / (t / 8227.0 + n / 12241.0 + d / 22638.0) + 100.0);

        let mut output = (pulse_out + tnd_out) * 65535.0;

        for i in 0..3 {
            output = self.filters[i].tick(output);
        }

        // The final range is -32767 to +32767
        output as i16
    }

    pub fn write_register(&mut self, addr: u16, val: u8) {
        match addr {
            0x4000 => self.pulse1.write_main_register(val),
            0x4001 => self.pulse1.write_sweep_register(val),
            0x4002 => self.pulse1.write_timer_lo(val),
            0x4003 => self.pulse1.write_timer_hi_and_length(val),
            0x4004 => self.pulse2.write_main_register(val),
            0x4005 => self.pulse2.write_sweep_register(val),
            0x4006 => self.pulse2.write_timer_lo(val),
            0x4007 => self.pulse2.write_timer_hi_and_length(val),
            0x4008 => self.triangle.write_linear_counter_setup(val),
            0x4009 => (), // Unused, do nothing
            0x400A => self.triangle.write_timer_lo(val),
            0x400B => self.triangle.write_length_and_timer_hi(val),
            0x400C => self.noise.write_main_register(val),
            0x400D => (), // unused, do nothing
            0x400E => self.noise.write_mode_period_register(val),
            0x400F => self.noise.write_length_register(val),
            0x4010 => self.dmc.write_flags_rate(val),
            0x4011 => self.dmc.write_direct_load(val),
            0x4012 => self.dmc.write_sample_address(val),
            0x4013 => self.dmc.write_sample_length(val),
            0x4015 => {
                // ---D NT21
                // Enable DMC (D), noise (N), triangle (T), and pulse channels (2/1)
                self.pulse1.set_enabled(val & 0b0000_0001 != 0);
                self.pulse2.set_enabled(val & 0b0000_0010 != 0);
                self.triangle.set_enabled(val & 0b0000_0100 != 0);
                self.noise.set_enabled(val & 0b0000_1000 != 0);
                self.dmc.set_enabled(val & 0b0001_0000 != 0);
            }
            0x4017 => {
                self.frame_counter.write_register(val);

                // Writing to $4017 with bit 7 set ($80) will immediately clock all of its controlled units at the beginning of the 5-step sequence
                // with bit 7 clear, only the sequence is reset without clocking any of its units.
                if val & 0b1000_0000 != 0 {
                    self.clock_half_frame();
                }
            }
            _ => panic!(
                "writing to unexcepted address {:#x} (value: {:#b})",
                addr, val
            ),
        }
    }

    pub fn read_register(&mut self, addr: u16) -> u8 {
        match addr {
            0x4015 => {
                // IF-D NT21
                // I: DMC interrupt F: Frame interrupt D: DMC active NT21: Length counter > 0
                let result = (self.dmc.irq_flag as u8) << 7
                    | (self.frame_counter.irq_flag as u8) << 6
                    | (self.dmc.is_playing() as u8) << 4
                    | (self.noise.is_playin() as u8) << 3
                    | (self.triangle.is_playing() as u8) << 2
                    | (self.pulse2.is_playing() as u8) << 1
                    | self.pulse1.is_playing() as u8;

                self.frame_counter.irq_flag = false;

                result
            }
            _ => panic!("reading from unexcepted address {:#x}", addr),
        }
    }

    pub fn output(&mut self) -> Sound {
        let s = Sound {
            buffer: self.buffer.clone(),
        };

        self.buffer.clear();
        s
    }
}

pub struct Sound {
    pub buffer: Vec<i16>,
}
