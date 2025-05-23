mod apu;
mod bus;
mod cpu;
mod joypad;
mod mem;
mod opcodes;
mod ppu;
mod render;
mod rom;
mod trace;

use std::collections::HashMap;
use std::env;

use apu::APU;
use bus::Bus;
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
use sdl2::pixels::PixelFormatEnum;
use trace::trace;
use trace::trace2;
use tracing::{debug, trace, Level};

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

    let mut canvas = window
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .unwrap();
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

    let args: Vec<String> = env::args().collect();

    //load the game
    let raw = std::fs::read(&args[1]).unwrap();
    let rom = Rom::new(&raw).unwrap();

    let mut frame = Frame::new();

    let bus = Bus::new(rom, move |ppu: &PPU, apu: &mut APU, joypad: &mut Joypad| {
        render::render(ppu, &mut frame);
        texture.update(None, &frame.data, 256 * 3).unwrap();

        canvas.clear();
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

    /* Initialize Logger */
    let log = std::sync::Arc::new(std::fs::File::create("logs/trace.log").unwrap());

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
    cpu.run();

    cpu.run_with_callback(move |cpu| {
        tracing::info!("{}", trace2(cpu));
    });
}
