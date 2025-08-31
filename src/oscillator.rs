// src/oscillator.rs

use core::f32::consts::PI;
use libm::sinf;

// --- Constants ---
pub const SAMPLE_RATE: f32 = 48_000.0;
const TWO_PI: f32 = 2.0 * PI;

// --- Oscillator State ---
pub struct Oscillator {
    phase: f32,
    frequency: f32,
    amplitude: f32,
    wave_shape: f32,
}

impl Oscillator {
    pub const fn new() -> Self {
        Self {
            phase: 0.0,
            frequency: 440.0,
            amplitude: 0.5,
            wave_shape: 0.0,
        }
    }

    pub fn set_params(&mut self, frequency: f32, amplitude: f32, wave_shape: f32) {
        self.frequency = frequency;
        self.amplitude = amplitude;
        self.wave_shape = wave_shape;
    }

    pub fn next_sample(&mut self) -> f32 {
        let sin_val = sinf(self.phase);
        let tri_val = if self.phase < PI {
            -1.0 + (2.0 * self.phase / PI)
        } else {
            1.0 - (2.0 * (self.phase - PI) / PI)
        };
        let sqr_val = if self.phase < PI { 1.0 } else { -1.0 };

        let sample = if self.wave_shape < 1.0 {
            (1.0 - self.wave_shape) * sin_val + self.wave_shape * tri_val
        } else {
            (2.0 - self.wave_shape) * tri_val + (self.wave_shape - 1.0) * sqr_val
        };

        self.phase += TWO_PI * self.frequency / SAMPLE_RATE;
        while self.phase >= TWO_PI {
            self.phase -= TWO_PI;
        }
        sample * self.amplitude
    }
}
