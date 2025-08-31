// src/oscillator.rs

use core::f32::consts::PI;

// --- Constants ---
pub const SAMPLE_RATE: f32 = 48_000.0;
const TWO_PI: f32 = 2.0 * PI;

// --- Oscillator State ---
pub struct Oscillator {
    phase: f32,
    frequency: f32,
    amplitude: f32,
}

impl Oscillator {
    pub const fn new() -> Self {
        Self {
            phase: 0.0,
            frequency: 440.0,
            amplitude: 0.5,
        }
    }

    pub fn set_params(&mut self, frequency: f32, amplitude: f32) {
        self.frequency = frequency;
        self.amplitude = amplitude;
    }

    pub fn next_sample(&mut self) -> f32 {
        let sample = if self.phase < PI {
            -1.0 + (2.0 * self.phase / PI)
        } else {
            1.0 - (2.0 * (self.phase - PI) / PI)
        };
        self.phase += TWO_PI * self.frequency / SAMPLE_RATE;
        while self.phase >= TWO_PI {
            self.phase -= TWO_PI;
        }
        sample * self.amplitude
    }
}
