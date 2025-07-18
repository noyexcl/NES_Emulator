use std::rc::Rc;

use tracing::debug;

use crate::{apu::APU, joypad::Joypad, mem::Mem, ppu::PPU, rom::Rom, trace::Inspector};

//  _______________ $10000  _______________
// | PRG-ROM       |       |               |
// | Upper Bank    |       |               |
// |_ _ _ _ _ _ _ _| $C000 | PRG-ROM       |
// | PRG-ROM       |       |               |
// | Lower Bank    |       |               |
// |_______________| $8000 |_______________|
// | SRAM          |       | SRAM          |
// |_______________| $6000 |_______________|
// | Expansion ROM |       | Expansion ROM |
// |_______________| $4020 |_______________|
// | I/O Registers |       |               |
// |_ _ _ _ _ _ _ _| $4000 |               |
// | Mirrors       |       | I/O Registers |
// | $2000-$2007   |       |               |
// |_ _ _ _ _ _ _ _| $2008 |               |
// | I/O Registers |       |               |
// |_______________| $2000 |_______________|
// | Mirrors       |       |               |
// | $0000-$07FF   |       |               |
// |_ _ _ _ _ _ _ _| $0800 |               |
// | RAM           |       | RAM           |
// |_ _ _ _ _ _ _ _| $0200 |               |
// | Stack         |       |               |
// |_ _ _ _ _ _ _ _| $0100 |               |
// | Zero Page     |       |               |
// |_______________| $0000 |_______________|

#[allow(clippy::type_complexity)]
pub struct Bus<'call> {
    cpu_vram: [u8; 2048],
    wram: [u8; 2048], // TODO: implement this
    rom: Rc<Rom>,
    ppu: PPU,
    pub apu: APU,
    joypad: Joypad,
    open_bus: u8,

    pub cycles: usize,
    pub cpu_stall: usize,
    gameloop_callback: Box<dyn FnMut(&PPU, &mut APU, &mut Joypad) + 'call>,
}

impl<'call> Bus<'call> {
    pub fn new<F>(rom: Rom, gameloop_callback: F) -> Bus<'call>
    where
        F: FnMut(&PPU, &mut APU, &mut Joypad) + 'call,
    {
        let ppu = PPU::new(rom.chr_rom.clone(), rom.screen_mirroring);

        let rom = Rc::new(rom);
        let apu = APU::new(rom.clone());

        Bus {
            cpu_vram: [0; 2048],
            wram: [0; 2048],
            rom,
            ppu,
            apu,
            joypad: Joypad::new(),
            cycles: 0,
            cpu_stall: 0,
            gameloop_callback: Box::from(gameloop_callback),

            open_bus: 0,
        }
    }

    pub fn tick(&mut self, cycles: usize) {
        self.cycles += cycles;

        let mut frame_ready = false;

        for _ in 0..cycles {
            for _ in 0..3 {
                if self.ppu.tick() {
                    frame_ready = true;
                }
            }

            self.apu.tick();
        }

        self.cpu_stall = self.apu.cpu_stall;
        self.apu.cpu_stall = 0;

        if frame_ready {
            (self.gameloop_callback)(&self.ppu, &mut self.apu, &mut self.joypad);
        }
    }

    pub fn poll_nmi_status(&mut self) -> bool {
        self.ppu.poll_nmi_interrupt()
    }

    pub fn poll_irq_status(&self) -> bool {
        self.apu.poll_irq_status()
    }

    /// Reset PPU & APU state, and clock them a certain number of times before first instruction begins.
    pub fn reset(&mut self) {
        self.cycles = 8;

        for _ in 0..=8 {
            for _ in 0..3 {
                self.ppu.tick();
            }
        }

        self.apu.reset();
    }

    pub fn get_ppu_position(&self) -> (usize, usize) {
        (self.ppu.scanline, self.ppu.cycles)
    }
}

const RAM: u16 = 0x0000;
const RAM_MIRRORS_END: u16 = 0x1FFF;
const PPU_REGISTERS: u16 = 0x2000;
const PPU_REGISTERS_MIRRORS_END: u16 = 0x3FFF;

impl Mem for Bus<'_> {
    fn mem_read(&mut self, addr: u16) -> u8 {
        self.open_bus = match addr {
            RAM..=RAM_MIRRORS_END => {
                let mirror_down_addr = addr & 0b0000_0111_1111_1111;
                self.cpu_vram[mirror_down_addr as usize]
            }

            0x2000..=0x2007 => self.ppu.read_port(addr),

            0x4000..=0x4014 => 0x00, // TODO: Open bus
            0x4015 => self.apu.read_register(addr),
            0x4016 => self.joypad.read(),
            0x4017 => 0, // Ignore Joypad 2
            0x6000..=0x7FFF => self.wram[(addr - 0x6000) as usize],
            0x2008..=PPU_REGISTERS_MIRRORS_END => {
                let mirror_down_addr = addr & 0b0010_0000_0000_0111;
                self.mem_read(mirror_down_addr)
            }
            0x8000..=0xFFFF => self.rom.read_prg_rom(addr),
            _ => self.open_bus,
        };

        self.open_bus
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        match addr {
            RAM..=RAM_MIRRORS_END => {
                let mirror_down_addr = addr & 0b0000_0111_1111_1111;
                self.cpu_vram[mirror_down_addr as usize] = data;
            }

            0x2000..=0x2007 => self.ppu.write_to_port(addr, data),

            0x4000..=0x4013 | 0x4015 | 0x4017 => self.apu.write_register(addr, data),

            0x4016 => self.joypad.write(data), // Ignore Joypad 1

            0x4014 => {
                let mut buffer: [u8; 256] = [0; 256];
                let hi: u16 = (data as u16) << 8;
                for i in 0..256u16 {
                    buffer[i as usize] = self.mem_read(hi + i);
                }

                self.ppu.write_oam_dma(&buffer);

                // todo: handle this eventually
                let add_cycles = if self.cycles % 2 == 1 { 514 } else { 513 };
                self.tick(add_cycles); //todo this will cause weird effects as PPU will have 513/514 * 3 ticks
            }

            0x2008..=PPU_REGISTERS_MIRRORS_END => {
                let mirror_down_addr = addr & 0b0010_0000_0000_0111;
                self.mem_write(mirror_down_addr, data);
            }
            0x6000..=0x7FFF => self.wram[(addr - 0x6000) as usize] = data,
            0x8000..=0xFFFF => {
                debug!(
                    "Attempted to write to Cartridge ROM space {:02X}, value: {:02X}",
                    addr, data
                );
            }
            _ => {
                println!("Ignoring mem access(write) at {:x}", addr);
            }
        }

        self.open_bus = data;
    }
}

impl Inspector for Bus<'_> {
    fn inspect(&self, addr: u16) -> u8 {
        match addr {
            RAM..=RAM_MIRRORS_END => {
                let mirror_down_addr = addr & 0b0000_0111_1111_1111;
                self.cpu_vram[mirror_down_addr as usize]
            }
            0x2000..=0x2007 => self.ppu.inspect(addr),
            0x4000..=0x4014 => 0xFF,
            0x4015 => self.apu.inspect(addr),
            0x4016 | 0x4017 => 0xFF, // Ignore Joypads
            0x6000..=0x7FFF => self.wram[(addr - 0x6000) as usize],
            0x2008..=PPU_REGISTERS_MIRRORS_END => {
                let mirror_down_addr = addr & 0b0010_0000_0000_0111;
                self.inspect(mirror_down_addr)
            }
            0x8000..=0xFFFF => self.rom.read_prg_rom(addr),
            _ => {
                println!("Ignoring mem access(read) at {:x}", addr);
                0xFF
            }
        }
    }

    /// Get current state of the address as u16
    /// This function is intended to be used to log the value of the address
    fn inspect_u16(&self, addr: u16) -> u16 {
        let lo = self.inspect(addr) as u16;
        let hi = self.inspect(addr + 1) as u16;

        (hi << 8) | lo
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::rom::test;

    #[test]
    fn test_mem_read_write_to_ram() {
        let mut bus = Bus::new(test::TestRom::create_test_rom(vec![]), |_, _, _| {});
        bus.mem_write(0x01, 0x55);
        assert_eq!(bus.mem_read(0x01), 0x55);
    }
}
