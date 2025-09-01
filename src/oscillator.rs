// src/oscillator.rs

use core::f32::consts::PI;
use libm::sinf;
use micromath::F32Ext;

// --- Constants ---
// Match this with the sample rate configured in Cargo.toml
pub const SAMPLE_RATE: f32 = 96_000.0;
const TWO_PI: f32 = 2.0 * PI;

// --- Oscillator State ---
pub struct Oscillator {
    phase: f32,
    frequency: f32,
    amplitude: f32,
    wave_shape: f32,
    fold_gain: f32,
    pm_amount: f32,
}

impl Oscillator {
    pub const fn new() -> Self {
        Self {
            phase: 0.0,
            frequency: 440.0,
            amplitude: 0.5,
            wave_shape: 0.0,
            fold_gain: 1.0,
            pm_amount: 0.0,
        }
    }

    pub fn set_params(
        &mut self,
        frequency: f32,
        amplitude: f32,
        wave_shape: f32,
        fold_gain: f32,
        pm_amount: f32,
    ) {
        self.frequency = frequency;
        self.amplitude = amplitude;
        self.wave_shape = wave_shape;
        self.fold_gain = fold_gain;
        self.pm_amount = pm_amount;
    }

    pub fn next_sample(&mut self) -> f32 {
        // Create modulator and apply phase modulation
        let modulator = sinf(self.phase);
        let modulated_phase = self.phase + modulator * self.pm_amount;
        let wrapped_phase = modulated_phase.rem_euclid(TWO_PI);

        let sin_val = sinf(modulated_phase);
        let tri_val = if wrapped_phase < PI {
            -1.0 + (2.0 * wrapped_phase / PI)
        } else {
            1.0 - (2.0 * (wrapped_phase - PI) / PI)
        };
        let sqr_val = if wrapped_phase < PI { 1.0 } else { -1.0 };

        let mut sample = if self.wave_shape < 1.0 {
            (1.0 - self.wave_shape) * sin_val + self.wave_shape * tri_val
        } else {
            (2.0 - self.wave_shape) * tri_val + (self.wave_shape - 1.0) * sqr_val
        };

        // Apply wavefolding
        sample *= self.fold_gain;
        const FOLD_THRESHOLD: f32 = 1.0;
        for _ in 0..4 {
            if sample > FOLD_THRESHOLD {
                sample = FOLD_THRESHOLD - (sample - FOLD_THRESHOLD);
            } else if sample < -FOLD_THRESHOLD {
                sample = -FOLD_THRESHOLD - (sample + FOLD_THRESHOLD);
            }
        }

        // Advance the main phase
        self.phase += TWO_PI * self.frequency / SAMPLE_RATE;
        while self.phase >= TWO_PI {
            self.phase -= TWO_PI;
        }
        sample * self.amplitude
    }
}
