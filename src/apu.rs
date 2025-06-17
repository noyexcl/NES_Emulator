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
use frame_counter::{FrameCounter, FrameType};
use noise::Noise;
use pulse::Pulse;
use triangle::Triangle;

use crate::{apu::filter::FilterChain, rom::Rom, trace::Inspector};

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug)]
pub struct APU {
    buffer: Vec<i16>,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    dmc: DMC,
    frame_counter: FrameCounter,
    cycles: usize,
    pub cpu_stall: usize,
    filter_chain: FilterChain,
    sample_period: f64,
    sample_counter: f64,
}

impl APU {
    pub fn new(rom: Rc<Rom>) -> Self {
        //let sample_period = 1_789_773.0 / 44_100.0;
        let sample_period = 40.52;

        Self {
            buffer: vec![],
            pulse1: Pulse::new(true),
            pulse2: Pulse::new(false),
            triangle: Triangle::new(),
            noise: Noise::new(),
            dmc: DMC::new(rom),
            frame_counter: FrameCounter::new(),
            filter_chain: FilterChain::new(44100.0),
            cycles: 0,
            cpu_stall: 0,
            sample_period,
            sample_counter: sample_period,
        }
    }

    pub fn tick(&mut self) {
        self.cycles += 1;

        self.triangle.clock();

        if self.cycles % 2 == 1 {
            self.pulse1.apu_tick();
            self.pulse2.apu_tick();
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

        self.pulse1.length_counter.reload();
        self.pulse2.length_counter.reload();
        self.triangle.length_counter.reload();
        self.noise.length_counter.reload();

        self.sample();

        self.cpu_stall = self.dmc.cpu_stall;
        self.dmc.cpu_stall = 0;
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

    pub fn reset(&mut self) {
        self.frame_counter.write_register(0x00);
        for _ in 0..10 {
            self.tick();
        }
    }

    pub fn sample(&mut self) {
        let pulse1 = self.pulse1.sample() as f32;
        let pulse2 = self.pulse2.sample() as f32;
        let t = self.triangle.sample() as f32;
        let n = self.noise.sample() as f32;
        let d = self.dmc.sample() as f32;

        let pulse_out = 95.88 / ((8218.0 / (pulse1 + pulse2)) + 100.0);
        let tnd_out = 159.79 / ((1.0 / (t / 8227.0 + n / 12241.0 + d / 22638.0)) + 100.0);

        let output = pulse_out + tnd_out;

        self.filter_chain.consume(output);
        self.sample_counter -= 1.0;

        if self.sample_counter <= 1.0 {
            // The final range is -32767 to +32767
            let sample = (self.filter_chain.output() * 65535.0) as i16;
            self.buffer.push(sample);
            self.buffer.push(sample);

            self.sample_counter += self.sample_period;
        }
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
                    | (self.noise.is_playing() as u8) << 3
                    | (self.triangle.is_playing() as u8) << 2
                    | (self.pulse2.is_playing() as u8) << 1
                    | self.pulse1.is_playing() as u8;

                self.frame_counter.irq_flag = false;

                result
            }
            _ => panic!("reading from unexcepted address {:#x}", addr),
        }
    }

    pub fn poll_irq_status(&self) -> bool {
        self.frame_counter.irq_flag | self.dmc.irq_flag
    }

    pub fn output(&mut self) -> Sound {
        let s = Sound {
            buffer: self.buffer.clone(),
        };

        self.buffer.clear();
        s
    }
}

impl Inspector for APU {
    fn inspect(&self, addr: u16) -> u8 {
        match addr {
            0x4015 => {
                // IF-D NT21
                // I: DMC interrupt F: Frame interrupt D: DMC active NT21: Length counter > 0
                (self.dmc.irq_flag as u8) << 7
                    | (self.frame_counter.irq_flag as u8) << 6
                    | (self.dmc.is_playing() as u8) << 4
                    | (self.noise.is_playing() as u8) << 3
                    | (self.triangle.is_playing() as u8) << 2
                    | (self.pulse2.is_playing() as u8) << 1
                    | self.pulse1.is_playing() as u8
            }
            _ => panic!("reading from unexcepted address {:#x}", addr),
        }
    }

    #[allow(unused)]
    fn inspect_u16(&self, addr: u16) -> u16 {
        panic!("There is no need to use this function for APU");
    }
}

pub struct Sound {
    pub buffer: Vec<i16>,
}

#[rustfmt::skip]
pub static PULSE_TABLE: [f32; 31] = [
    0.0,          0.011_609_139, 0.022_939_48, 0.034_000_948, 0.044_803,    0.055_354_66,
    0.065_664_53, 0.075_740_82,  0.085_591_4,  0.095_223_75,  0.104_645_04, 0.113_862_15,
    0.122_881_64, 0.131_709_8,   0.140_352_64, 0.148_815_96,  0.157_105_25, 0.165_225_88,
    0.173_182_92, 0.180_981_26,  0.188_625_59, 0.196_120_46,  0.203_470_17, 0.210_678_94,
    0.217_750_76, 0.224_689_5,   0.231_498_87, 0.238_182_47,  0.244_743_78, 0.251_186_07,
    0.257_512_57,
];

#[rustfmt::skip]
pub static TND_TABLE: [f32; 203] = [
    0.0,           0.006_699_824, 0.013_345_02,  0.019_936_256, 0.026_474_18,  0.032_959_443,
    0.039_392_676, 0.045_774_5,   0.052_105_535, 0.058_386_38,  0.064_617_634, 0.070_799_87,
    0.076_933_69,  0.083_019_62,  0.089_058_26,  0.095_050_134, 0.100_995_794, 0.106_895_77,
    0.112_750_58,  0.118_560_754, 0.124_326_79,  0.130_049_18,  0.135_728_45,  0.141_365_05,
    0.146_959_5,   0.152_512_22,  0.158_023_7,   0.163_494_4,   0.168_924_76,  0.174_315_24,
    0.179_666_28,  0.184_978_3,   0.190_251_74,  0.195_486_98,  0.200_684_47,  0.205_844_63,
    0.210_967_81,  0.216_054_44,  0.221_104_92,  0.226_119_6,   0.231_098_88,  0.236_043_11,
    0.240_952_72,  0.245_828_,    0.250_669_36,  0.255_477_1,   0.260_251_64,  0.264_993_28,
    0.269_702_37,  0.274_379_22,  0.279_024_18,  0.283_637_58,  0.288_219_72,  0.292_770_95,
    0.297_291_52,  0.301_781_8,   0.306_242_1,   0.310_672_67,  0.315_073_85,  0.319_445_88,
    0.323_789_12,  0.328_103_78,  0.332_390_2,   0.336_648_6,   0.340_879_3,   0.345_082_55,
    0.349_258_63,  0.353_407_77,  0.357_530_27,  0.361_626_36,  0.365_696_34,  0.369_740_37,
    0.373_758_76,  0.377_751_74,  0.381_719_56,  0.385_662_44,  0.389_580_64,  0.393_474_37,
    0.397_343_84,  0.401_189_3,   0.405_011_,    0.408_809_07,  0.412_583_83,  0.416_335_46,
    0.420_064_15,  0.423_770_13,  0.427_453_6,   0.431_114_76,  0.434_753_84,  0.438_370_97,
    0.441_966_44,  0.445_540_4,   0.449_093_,    0.452_624_53,  0.456_135_06,  0.459_624_9,
    0.463_094_12,  0.466_542_93,  0.469_971_57,  0.473_380_15,  0.476_768_94,  0.480_137_94,
    0.483_487_52,  0.486_817_7,   0.490_128_73,  0.493_420_7,   0.496_693_88,  0.499_948_32,
    0.503_184_26,  0.506_401_84,  0.509_601_2,   0.512_782_45,  0.515_945_85,  0.519_091_4,
    0.522_219_5,   0.525_330_07,  0.528_423_25,  0.531_499_3,   0.534_558_36,  0.537_600_5,
    0.540_625_93,  0.543_634_8,   0.546_627_04,  0.549_603_04,  0.552_562_83,  0.555_506_47,
    0.558_434_3,   0.561_346_23,  0.564_242_5,   0.567_123_23,  0.569_988_5,   0.572_838_4,
    0.575_673_2,   0.578_492_94,  0.581_297_7,   0.584_087_6,   0.586_862_8,   0.589_623_45,
    0.592_369_56,  0.595_101_36,  0.597_818_9,   0.600_522_3,   0.603_211_6,   0.605_887_,
    0.608_548_64,  0.611_196_6,   0.613_830_8,   0.616_451_56,  0.619_059_,    0.621_653_14,
    0.624_234_,    0.626_801_85,  0.629_356_7,   0.631_898_64,  0.634_427_7,   0.636_944_2,
    0.639_448_05,  0.641_939_34,  0.644_418_24,  0.646_884_86,  0.649_339_2,   0.651_781_4,
    0.654_211_5,   0.656_629_74,  0.659_036_04,  0.661_430_6,   0.663_813_4,   0.666_184_66,
    0.668_544_35,  0.670_892_6,   0.673_229_46,  0.675_555_05,  0.677_869_44,  0.680_172_74,
    0.682_464_96,  0.684_746_2,   0.687_016_6,   0.689_276_2,   0.691_525_04,  0.693_763_3,
    0.695_990_9,   0.698_208_03,  0.700_414_8,   0.702_611_1,   0.704_797_2,   0.706_973_1,
    0.709_138_8,   0.711_294_5,   0.713_440_1,   0.715_575_9,   0.717_701_8,   0.719_817_9,
    0.721_924_25,  0.724_020_96,  0.726_108_,    0.728_185_65,  0.730_253_8,   0.732_312_56,
    0.734_361_95,  0.736_402_1,   0.738_433_1,   0.740_454_9,   0.742_467_6,
];
