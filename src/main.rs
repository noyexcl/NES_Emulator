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
use std::fs::File;
use std::io::Write;

use apu::Sound;
use apu::APU;
use bus::Bus;
use cpu::Mem;
use cpu::CPU;
use joypad::Joypad;
use joypad::JoypadButton;
use ppu::PPU;
use rand::Rng;
use render::frame::show_tile;
use render::frame::Frame;
use rom::Rom;
use sdl2::audio;
use sdl2::audio::AudioQueue;
use sdl2::audio::{AudioCallback, AudioSpecDesired};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::pixels::PixelFormatEnum;
use sdl2::EventPump;
use std::time::Duration;
use trace::trace;

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

impl AudioCallback for Sound {
    type Channel = i16;

    fn callback(&mut self, out: &mut [Self::Channel]) {
        println!("out {}, buf {}", out.len(), self.buffer.len());

        for x in out.iter_mut() {
            if let Some(data) = self.buffer.pop() {
                *x = data;
            } else {
                *x = 0;
            }
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
        freq: Some(43800),
        channels: Some(2),
        samples: Some(730),
    };

    let audio_queue: AudioQueue<i16> = audio_subsystem.open_queue(None, &desired_spec).unwrap();
    audio_queue.resume();

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

    //load the game
    let raw = std::fs::read("pacman.nes").unwrap();
    let rom = Rom::new(&raw).unwrap();

    let mut frame = Frame::new();

    let bus = Bus::new(rom, move |ppu: &PPU, apu: &mut APU, joypad: &mut Joypad| {
        render::render(ppu, &mut frame);
        texture.update(None, &frame.data, 256 * 3).unwrap();

        canvas.copy(&texture, None, None).unwrap();
        canvas.present();

        let sound = apu.output();
        audio_queue.queue(&sound.buffer);

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => std::process::exit(0),

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

    let mut log_file = File::create("cpu.log").unwrap();

    let mut cpu = CPU::new(bus);
    cpu.reset();
    //cpu.program_counter = 0xc000;
    cpu.run();

    /*
    cpu.run_with_callback(move |cpu| {
        let log = trace(cpu) + "\n";
        // Write log into nex.log File
        log_file.write_all(log.as_bytes()).unwrap();
        log_file.flush().unwrap();
    });
    */

    /*
    let mut log_file = File::create("cpu.log").unwrap();
    let raw = std::fs::read("nestest.nes").unwrap();
    let rom = Rom::new(&raw).unwrap();
    let bus = Bus::new(rom, |_, _| {});

    let mut cpu = CPU::new(bus);
    cpu.reset();
    cpu.program_counter = 0xc000;
    */

    /*

    let mut screen_state = [0; 32 * 3 * 32];
    let mut rng = rand::thread_rng();

    // run the game cycle
    cpu.run_with_callback(move |cpu| {
        // Logging CPU Status
        let log = trace(cpu) + "\n";
        log_file.write_all(log.as_bytes()).unwrap();
        log_file.flush().unwrap();

        handle_user_input(cpu, &mut event_pump);

        cpu.mem_write(0xfe, rng.gen_range(1, 16));

        if read_screen_state(cpu, &mut screen_state) {
            texture.update(None, &screen_state, 32 * 3).unwrap();

            canvas.copy(&texture, None, None).unwrap();

            canvas.present();
        }

        ::std::thread::sleep(std::time::Duration::new(0, 70_000));
    });
    */

    /*
    let mut file = File::create("nes.log").unwrap();
    let raw = std::fs::read("nestest.nes").unwrap();
    let rom = Rom::new(&raw).unwrap();
    let bus = Bus::new(rom, |_| {});

    let mut cpu = CPU::new(bus);
    cpu.reset();
    cpu.program_counter = 0xc000;

    cpu.run_with_callback(move |cpu| {
        let log = trace(cpu) + "\n";
        // Write log into nex.log File
        file.write_all(log.as_bytes()).unwrap();
        file.flush().unwrap();
    })
    */
}
