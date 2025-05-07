mod apu;
mod bus;
mod cpu;
mod joypad;
mod opcodes;
mod ppu;
mod render;
mod rom;
mod trace;

use std::collections::HashMap;
use std::time::Instant;

use apu::APU;
use bus::Bus;
use cpu::Mem;
use cpu::CPU;
use joypad::Joypad;
use joypad::JoypadButton;
use ppu::PPU;
use render::frame::Frame;
use rom::Rom;
use sdl2::audio::AudioQueue;
use sdl2::audio::AudioSpecDesired;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::pixels::PixelFormatEnum;
use sdl2::EventPump;
use std::time::Duration;
use trace::trace;
use tracing::debug;
use tracing::{trace, Level};

fn color(byte: u8) -> Color {
    match byte {
        0 => sdl2::pixels::Color::BLACK,
        1 => sdl2::pixels::Color::WHITE,
        2 | 9 => sdl2::pixels::Color::GREY,
        3 | 10 => sdl2::pixels::Color::RED,
        4 | 11 => sdl2::pixels::Color::GREEN,
        5 | 12 => sdl2::pixels::Color::BLUE,
        6 | 13 => sdl2::pixels::Color::MAGENTA,
        7 | 14 => sdl2::pixels::Color::YELLOW,
        _ => sdl2::pixels::Color::CYAN,
    }
}

fn read_screen_state(cpu: &mut CPU, frame: &mut [u8; 32 * 3 * 32]) -> bool {
    let mut frame_idx = 0;
    let mut update = false;
    for i in 0x0200..0x600 {
        let color_idx = cpu.mem_read(i as u16);
        let (b1, b2, b3) = color(color_idx).rgb();
        if frame[frame_idx] != b1 || frame[frame_idx + 1] != b2 || frame[frame_idx + 2] != b3 {
            frame[frame_idx] = b1;
            frame[frame_idx + 1] = b2;
            frame[frame_idx + 2] = b3;
            update = true;
        }
        frame_idx += 3;
    }
    update
}

fn handle_user_input(cpu: &mut CPU, event_pump: &mut EventPump) {
    for event in event_pump.poll_iter() {
        match event {
            Event::Quit { .. }
            | Event::KeyDown {
                keycode: Some(Keycode::Escape),
                ..
            } => std::process::exit(0),
            Event::KeyDown {
                keycode: Some(Keycode::W),
                ..
            } => {
                cpu.mem_write(0xff, 0x77);
            }
            Event::KeyDown {
                keycode: Some(Keycode::S),
                ..
            } => {
                cpu.mem_write(0xff, 0x73);
            }
            Event::KeyDown {
                keycode: Some(Keycode::A),
                ..
            } => {
                cpu.mem_write(0xff, 0x61);
            }
            Event::KeyDown {
                keycode: Some(Keycode::D),
                ..
            } => {
                cpu.mem_write(0xff, 0x64);
            }
            _ => { /* do nothing */ }
        }
    }
}

fn main() {
    // init sdl2
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    let audio_subsystem = sdl_context.audio().unwrap();
    let window = video_subsystem
        .window("NES Emulator", (256.0 * 3.0) as u32, (240.0 * 3.0) as u32)
        .position_centered()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas().present_vsync().build().unwrap();
    let mut event_pump = sdl_context.event_pump().unwrap();
    canvas.set_scale(3.0, 3.0).unwrap();

    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_target(PixelFormatEnum::RGB24, 256, 240)
        .unwrap();

    let desired_spec = AudioSpecDesired {
        freq: Some(44100),
        channels: Some(2),
        samples: Some(735),
    };

    let audio_queue: AudioQueue<i16> = audio_subsystem.open_queue(None, &desired_spec).unwrap();
    audio_queue.resume();

    let mut samples: Vec<i16> = vec![];
    let wav_spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    // init joypad
    let mut key_map = HashMap::new();
    key_map.insert(Keycode::Down, JoypadButton::DOWN);
    key_map.insert(Keycode::Up, JoypadButton::UP);
    key_map.insert(Keycode::Right, JoypadButton::RIGHT);
    key_map.insert(Keycode::Left, JoypadButton::LEFT);
    key_map.insert(Keycode::Q, JoypadButton::SELECT);
    key_map.insert(Keycode::W, JoypadButton::START);
    key_map.insert(Keycode::A, JoypadButton::BUTTON_A);
    key_map.insert(Keycode::S, JoypadButton::BUTTON_B);

    let target_frame_time = Duration::from_secs_f64(1.0 / 60.0);
    let frame_start = Instant::now();

    //load the game
    let raw = std::fs::read("1-len_ctr.nes").unwrap();
    let rom = Rom::new(&raw).unwrap();

    let mut frame = Frame::new();

    let bus = Bus::new(rom, move |ppu: &PPU, apu: &mut APU, joypad: &mut Joypad| {
        if ppu.ctrl.generate_vblank_nmi() {
            render::render(ppu, &mut frame);
        }

        texture.update(None, &frame.data, 256 * 3).unwrap();

        /*
        let frame_time = frame_start.elapsed();
        if frame_time < target_frame_time {
            std::thread::sleep(target_frame_time - frame_time);
        }
        */

        canvas.copy(&texture, None, None).unwrap();
        canvas.present();

        let mut sound = apu.output();
        audio_queue.queue(&sound.buffer);

        samples.append(&mut sound.buffer.clone());

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => {
                    let mut writer = hound::WavWriter::create("output.wav", wav_spec).unwrap();
                    for s in samples.iter() {
                        writer.write_sample(*s).unwrap();
                    }

                    writer.finalize().unwrap();

                    std::process::exit(0)
                }

                Event::KeyDown { keycode, .. } => {
                    if let Some(key) = key_map.get(&keycode.unwrap_or(Keycode::Ampersand)) {
                        joypad.set_button_pressed_status(*key, true);
                    }
                }
                Event::KeyUp { keycode, .. } => {
                    if let Some(key) = key_map.get(&keycode.unwrap_or(Keycode::Ampersand)) {
                        joypad.set_button_pressed_status(*key, false);
                    }
                }

                _ => { /* do nothing */ }
            }
        }
    });

    /* Initialize Logger */
    let log = std::sync::Arc::new(std::fs::File::create("trace.log").unwrap());

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(log)
        .without_time()
        .with_level(false)
        .with_target(false)
        .with_max_level(Level::TRACE)
        .init();

    let mut cpu = CPU::new(bus);
    cpu.reset();
    // cpu.program_counter = 0xc000;
    // cpu.run();

    cpu.run_with_callback(move |cpu| {
        tracing::trace!("{}", trace(cpu));
    });
}
