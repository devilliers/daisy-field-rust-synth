// src/display.rs

use core::f32::consts::PI;
use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle},
};
use libm::sinf;
use ssd1306::{mode::BufferedGraphicsMode, prelude::*, Ssd1306};
use stm32h7xx_hal as hal;

// Type alias for our specific display object to make function signatures cleaner
pub type OledDisplay = Ssd1306<
    display_interface_spi::SPIInterface<
        stm32h7xx_hal::spi::Spi<stm32h7xx_hal::pac::SPI1, hal::spi::Enabled>,
        stm32h7xx_hal::gpio::Pin<'B', 4, stm32h7xx_hal::gpio::Output>,
        stm32h7xx_hal::gpio::Pin<'G', 10, stm32h7xx_hal::gpio::Output>,
    >,
    DisplaySize128x64,
    BufferedGraphicsMode<DisplaySize128x64>,
>;

// Display constants
const DISPLAY_WIDTH: i32 = 128;
const DISPLAY_HEIGHT: i32 = 64;
const DISPLAY_CENTER_Y: i32 = DISPLAY_HEIGHT / 2;
const TWO_PI: f32 = 2.0 * PI;

fn triangle_wave(phase: f32) -> f32 {
    let mut phase = phase;
    while phase < 0.0 {
        phase += TWO_PI;
    }
    while phase >= TWO_PI {
        phase -= TWO_PI;
    }

    if phase < PI {
        -1.0 + (2.0 * phase / PI)
    } else {
        1.0 - (2.0 * (phase - PI) / PI)
    }
}

fn square_wave(phase: f32) -> f32 {
    let mut phase = phase;
    while phase < 0.0 {
        phase += TWO_PI;
    }
    while phase >= TWO_PI {
        phase -= TWO_PI;
    }

    if phase < PI {
        1.0
    } else {
        -1.0
    }
}

pub fn draw_waveform(
    display: &mut OledDisplay,
    smoothed_freq: f32,
    smoothed_amp: f32,
    min_freq: f32,
    max_freq_range: f32,
    wave_shape: f32,
) {
    const MIN_CYCLES: f32 = 1.0;
    const MAX_CYCLES: f32 = 10.0;

    display.clear(BinaryColor::Off).unwrap();

    let normalized_freq = (smoothed_freq - min_freq) / max_freq_range;
    let cycles_on_screen =
        MIN_CYCLES + normalized_freq.max(0.0).min(1.0) * (MAX_CYCLES - MIN_CYCLES);

    for x in 0..(DISPLAY_WIDTH - 1) {
        let phase1 = (x as f32 / DISPLAY_WIDTH as f32) * TWO_PI * cycles_on_screen;
        let phase2 = ((x + 1) as f32 / DISPLAY_WIDTH as f32) * TWO_PI * cycles_on_screen;

        let sin_val1 = sinf(phase1);
        let tri_val1 = triangle_wave(phase1);
        let sqr_val1 = square_wave(phase1);

        let sin_val2 = sinf(phase2);
        let tri_val2 = triangle_wave(phase2);
        let sqr_val2 = square_wave(phase2);

        let val1 = if wave_shape < 1.0 {
            (1.0 - wave_shape) * sin_val1 + wave_shape * tri_val1
        } else {
            (2.0 - wave_shape) * tri_val1 + (wave_shape - 1.0) * sqr_val1
        };

        let val2 = if wave_shape < 1.0 {
            (1.0 - wave_shape) * sin_val2 + wave_shape * tri_val2
        } else {
            (2.0 - wave_shape) * tri_val2 + (wave_shape - 1.0) * sqr_val2
        };

        let y1 = DISPLAY_CENTER_Y - (val1 * smoothed_amp * (DISPLAY_CENTER_Y - 1) as f32) as i32;
        let y2 = DISPLAY_CENTER_Y - (val2 * smoothed_amp * (DISPLAY_CENTER_Y - 1) as f32) as i32;

        Line::new(Point::new(x, y1), Point::new(x + 1, y2))
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
            .draw(display)
            .unwrap();
    }

    display.flush().unwrap();
}
