use crate::ppu::palette;

#[derive(Debug)]
pub struct Frame {
    pub buffer: Vec<u8>,
    pub pixel_buffer: Vec<u8>,
}

impl Frame {
    const WIDTH: usize = 256;
    const HIGHT: usize = 240;

    pub fn new() -> Self {
        Frame {
            buffer: vec![0; (Frame::WIDTH) * (Frame::HIGHT) * 3],
            pixel_buffer: vec![0; (Frame::WIDTH) * (Frame::HIGHT)],
        }
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, color: u8) {
        self.pixel_buffer[y * Frame::WIDTH + x] = color;

        let rgb = palette::SYSTEM_PALETTE[color as usize];

        let base = y * 3 * Frame::WIDTH + x * 3;
        if base + 2 < self.buffer.len() {
            self.buffer[base] = rgb.0;
            self.buffer[base + 1] = rgb.1;
            self.buffer[base + 2] = rgb.2;
        }
    }

    #[allow(unused, reason = "This is used for testing.")]
    pub fn get_pixel(&self, x: usize, y: usize) -> u8 {
        self.pixel_buffer[y * Frame::WIDTH + x]
    }
}
