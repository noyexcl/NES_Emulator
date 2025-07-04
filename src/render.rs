pub mod frame;
pub mod palette;

use crate::{ppu::PPU, rom::Mirroring};
use frame::Frame;

fn bg_palette(ppu: &PPU, attribute_table: &[u8], tile_column: usize, tile_row: usize) -> [u8; 4] {
    let attr_table_idx = tile_row / 4 * 8 + tile_column / 4;
    let attr_byte = attribute_table[attr_table_idx];

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

fn sprite_palette(ppu: &PPU, palette_idx: u8) -> [u8; 4] {
    let start = 0x11 + (palette_idx * 4) as usize;
    [
        0,
        ppu.palette_table[start],
        ppu.palette_table[start + 1],
        ppu.palette_table[start + 2],
    ]
}

struct Rect {
    x1: usize,
    y1: usize,
    x2: usize,
    y2: usize,
}

impl Rect {
    fn new(x1: usize, y1: usize, x2: usize, y2: usize) -> Self {
        Rect { x1, y1, x2, y2 }
    }
}

#[allow(clippy::needless_range_loop)]
fn render_name_table(
    ppu: &PPU,
    frame: &mut Frame,
    name_table: &[u8],
    view_port: Rect,
    shift_x: isize,
    shift_y: isize,
) {
    let bank = ppu.ctrl.background_pattern_addr();
    let attribute_table = &name_table[0x03c0..0x0400];

    for i in 0..0x03c0 {
        let tile_column = i % 32;
        let tile_row = i / 32;
        let tile_idx = name_table[i] as u16;
        let tile = ppu.get_tile_data(bank, tile_idx);
        let palette = bg_palette(ppu, attribute_table, tile_column, tile_row);

        for y in 0..=7 {
            let mut lower_bits = tile[y];
            let mut upper_bits = tile[y + 8];

            for x in (0..=7).rev() {
                let value = (1 & upper_bits) << 1 | (1 & lower_bits);
                upper_bits >>= 1;
                lower_bits >>= 1;
                let rgb = match value {
                    0 => palette::SYSTEM_PALETTE[ppu.palette_table[0] as usize],
                    1 => palette::SYSTEM_PALETTE[palette[1] as usize],
                    2 => palette::SYSTEM_PALETTE[palette[2] as usize],
                    3 => palette::SYSTEM_PALETTE[palette[3] as usize],
                    _ => panic!("can't be"),
                };
                let pixel_x = tile_column * 8 + x;
                let pixel_y = tile_row * 8 + y;

                if pixel_x >= view_port.x1
                    && pixel_x < view_port.x2
                    && pixel_y >= view_port.y1
                    && pixel_y < view_port.y2
                {
                    frame.set_pixel(
                        (shift_x + pixel_x as isize) as usize,
                        (shift_y + pixel_y as isize) as usize,
                        rgb,
                    );
                }
            }
        }
    }
}

pub fn render(ppu: &PPU, frame: &mut Frame) {
    let scroll_x = ppu.scroll.x as usize;
    let scroll_y = ppu.scroll.y as usize;

    let (main_nametable, second_nametable) = match (&ppu.mirroring, ppu.ctrl.base_nametable_addr())
    {
        (Mirroring::Vertical, 0x2000) | (Mirroring::Vertical, 0x2800) => {
            (&ppu.vram[0..0x400], &ppu.vram[0x400..0x800])
        }
        (Mirroring::Vertical, 0x2400) | (Mirroring::Vertical, 0x2C00) => {
            (&ppu.vram[0x400..0x800], &ppu.vram[0..0x400])
        }
        (Mirroring::Horizontal, 0x2000) | (Mirroring::Horizontal, 0x2400) => {
            (&ppu.vram[0..0x400], &ppu.vram[0x400..0x800])
        }
        (Mirroring::Horizontal, 0x2800) | (Mirroring::Horizontal, 0x2C00) => {
            (&ppu.vram[0x400..0x800], &ppu.vram[0..0x400])
        }
        (_, _) => {
            panic!("Unimplemented nametable mirroring: {:?}", ppu.mirroring);
        }
    };

    render_name_table(
        ppu,
        frame,
        main_nametable,
        Rect::new(scroll_x, scroll_y, 256, 240),
        -(scroll_x as isize),
        -(scroll_y as isize),
    );

    // Horizontal Mirroring | Vertical Mirroring
    // A A'                 | A  B
    // B B'                 | A' B'
    if scroll_x > 0 {
        // If it's horizontal mirroring and trying to scroll horizontal, use same table
        let table = match ppu.mirroring {
            Mirroring::Vertical => second_nametable,
            Mirroring::Horizontal => main_nametable,
            _ => unimplemented!(),
        };

        render_name_table(
            ppu,
            frame,
            table,
            Rect::new(0, 0, scroll_x, 240),
            (256 - scroll_x) as isize,
            0,
        );
    } else if scroll_y > 0 {
        // If it's vertical mirroring and trying to scroll vertical, use same table
        let table = match ppu.mirroring {
            Mirroring::Vertical => main_nametable,
            Mirroring::Horizontal => second_nametable,
            _ => unimplemented!(),
        };

        render_name_table(
            ppu,
            frame,
            table,
            Rect::new(0, 0, 256, scroll_y),
            0,
            (240 - scroll_y) as isize,
        );
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

        let tile = ppu.get_tile_data(bank, tile_idx);

        for y in 0..=7 {
            let mut lower_bits = tile[y];
            let mut upper_bits = tile[y + 8];
            'x: for x in (0..=7).rev() {
                let value = (1 & upper_bits) << 1 | (1 & lower_bits);
                upper_bits >>= 1;
                lower_bits >>= 1;

                let rgb = match value {
                    0 => continue 'x, // skip coloring the pixel because it's transparent
                    1 => palette::SYSTEM_PALETTE[sprite_palette[1] as usize],
                    2 => palette::SYSTEM_PALETTE[sprite_palette[2] as usize],
                    3 => palette::SYSTEM_PALETTE[sprite_palette[3] as usize],
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
