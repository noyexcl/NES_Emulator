pub mod frame;
pub mod palette;

use crate::ppu::NesPPU;
use frame::Frame;
use sdl2::render::TextureQuery;

use self::palette::SYSTEM_PALLETE;

fn bg_palette(ppu: &NesPPU, tile_column: usize, tile_row: usize) -> [u8; 4] {
    let attr_table_idx = tile_row / 4 * 8 + tile_column / 4;
    let attr_byte = ppu.vram[0x03c0 + attr_table_idx];

    let palette_idx = match (tile_column % 4 / 2, tile_row % 4 / 2) {
        (0, 0) => attr_byte & 0b11,
        (1, 0) => (attr_byte >> 2) & 0b11,
        (0, 1) => (attr_byte >> 4) & 0b11,
        (1, 1) => (attr_byte >> 6) & 0b11,
        _ => unreachable!(),
    };

    let palette_start: usize = 1 + (palette_idx as usize) * 4;
    [
        ppu.palette_table[0],
        ppu.palette_table[palette_start],
        ppu.palette_table[palette_start + 1],
        ppu.palette_table[palette_start + 2],
    ]
}

pub fn render(ppu: &NesPPU, frame: &mut Frame) {
    let bank = ppu.ctrl.background_pattern_addr();

    for i in 0..0x03c0 {
        let pattern_idx = ppu.vram[i] as u16;
        let tile_column = i % 32;
        let tile_row = i / 32;
        let tile = &ppu.chr_rom
            [(bank + pattern_idx * 16) as usize..=(bank + pattern_idx * 16 + 15) as usize];
        let palette = bg_palette(ppu, tile_column, tile_row);

        for y in 0..=7 {
            let mut lower_bits = tile[y];
            let mut upper_bits = tile[y + 8];

            for x in (0..=7).rev() {
                let value = (1 & upper_bits) << 1 | (1 & lower_bits);
                lower_bits = lower_bits >> 1;
                upper_bits = upper_bits >> 1;
                let rgb = match value {
                    0 => palette::SYSTEM_PALLETE[palette[0] as usize],
                    1 => palette::SYSTEM_PALLETE[palette[1] as usize],
                    2 => palette::SYSTEM_PALLETE[palette[2] as usize],
                    3 => palette::SYSTEM_PALLETE[palette[3] as usize],
                    _ => unreachable!("can't be"),
                };
                frame.set_pixel(tile_column * 8 + x, tile_row * 8 + y, rgb)
            }
        }
    }

    for i in (0..ppu.oam_data.len()).step_by(4).rev() {
        let tile_idx = ppu.oam_data[i + 1] as u16;
        let tile_x = ppu.oam_data[i + 3] as usize;
        let tile_y = ppu.oam_data[i] as usize;

        let flip_vertical = ppu.oam_data[i + 2] >> 7 & 1 == 1;
        let flip_horizontal = ppu.oam_data[i + 2] >> 6 & 1 == 1;
        let palette_idx = ppu.oam_data[i + 2] & 0b11;
        let sprite_palette = sprite_palette(ppu, palette_idx);

        let bank: u16 = ppu.ctrl.sprite_pattern_addr();

        let tile =
            &ppu.chr_rom[(bank + tile_idx * 16) as usize..=(bank + tile_idx * 16 + 15) as usize];

        for y in 0..=7 {
            let mut lower_bits = tile[y];
            let mut upper_bits = tile[y + 8];
            'x: for x in (0..=7).rev() {
                let value = (1 & upper_bits) << 1 | (1 & lower_bits);
                upper_bits = upper_bits >> 1;
                lower_bits = lower_bits >> 1;

                let rgb = match value {
                    0 => continue 'x, // skip coloring the pixel because it's transparent
                    1 => palette::SYSTEM_PALLETE[sprite_palette[1] as usize],
                    2 => palette::SYSTEM_PALLETE[sprite_palette[2] as usize],
                    3 => palette::SYSTEM_PALLETE[sprite_palette[3] as usize],
                    _ => unreachable!("can't be"),
                };
                match (flip_horizontal, flip_vertical) {
                    (false, false) => frame.set_pixel(tile_x + x, tile_y + y, rgb),
                    (true, false) => frame.set_pixel(tile_x + 7 - x, tile_y + y, rgb),
                    (false, true) => frame.set_pixel(tile_x + x, tile_y + 7 - y, rgb),
                    (true, true) => frame.set_pixel(tile_x + 7 - x, tile_y + 7 - y, rgb),
                }
            }
        }
    }
}

fn sprite_palette(ppu: &NesPPU, palette_idx: u8) -> [u8; 4] {
    let start = 0x11 + (palette_idx * 4) as usize;
    [
        0,
        ppu.palette_table[start],
        ppu.palette_table[start + 1],
        ppu.palette_table[start + 2],
    ]
}
