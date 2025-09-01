// src/hardware.rs

use cortex_m::asm;
use cortex_m::prelude::_embedded_hal_adc_OneShot;
use stm32h7xx_hal::{
    adc,
    gpio::{Analog, Output, Pin, PushPull},
    pac,
};

pub struct Knobs<'a> {
    mux_sel_0: &'a mut Pin<'C', 4, Output<PushPull>>,
    mux_sel_1: &'a mut Pin<'C', 1, Output<PushPull>>,
    mux_sel_2: &'a mut Pin<'A', 6, Output<PushPull>>,
    adc: &'a mut adc::Adc<pac::ADC1, adc::Enabled>,
    adc_pin: &'a mut Pin<'A', 3, Analog>,
    // Add state for smoothed knob values
    smoothed_freq: f32,
    smoothed_amp: f32,
    smoothed_wave_shape: f32,
    smoothed_fold_gain: f32,
    smoothed_pm_amount: f32,
}

impl<'a> Knobs<'a> {
    pub fn new(
        mux_sel_0: &'a mut Pin<'C', 4, Output<PushPull>>,
        mux_sel_1: &'a mut Pin<'C', 1, Output<PushPull>>,
        mux_sel_2: &'a mut Pin<'A', 6, Output<PushPull>>,
        adc: &'a mut adc::Adc<pac::ADC1, adc::Enabled>,
        adc_pin: &'a mut Pin<'A', 3, Analog>,
    ) -> Self {
        Self {
            mux_sel_0,
            mux_sel_1,
            mux_sel_2,
            adc,
            adc_pin,
            // Initialize smoothed values
            smoothed_freq: 0.0,
            smoothed_amp: 0.0,
            smoothed_wave_shape: 0.0,
            smoothed_fold_gain: 1.0,
            smoothed_pm_amount: 0.0,
        }
    }

    pub fn read(&mut self, knob: u8) -> Option<u32> {
        let _ = knob & 0b111; // Ensure index is within 0-7

        match knob {
            1 => {
                self.mux_sel_2.set_low();
                self.mux_sel_1.set_low();
                self.mux_sel_0.set_low();
            }
            2 => {
                self.mux_sel_2.set_low();
                self.mux_sel_1.set_high();
                self.mux_sel_0.set_high();
            }
            3 => {
                self.mux_sel_2.set_low();
                self.mux_sel_1.set_low();
                self.mux_sel_0.set_high();
            }
            4 => {
                self.mux_sel_2.set_high();
                self.mux_sel_1.set_low();
                self.mux_sel_0.set_low();
            }
            5 => {
                self.mux_sel_2.set_low();
                self.mux_sel_1.set_high();
                self.mux_sel_0.set_low();
            }
            6 => {
                self.mux_sel_2.set_high();
                self.mux_sel_1.set_low();
                self.mux_sel_0.set_high();
            }
            7 => {
                self.mux_sel_2.set_high();
                self.mux_sel_1.set_high();
                self.mux_sel_0.set_low();
            }
            8 => {
                self.mux_sel_2.set_high();
                self.mux_sel_1.set_high();
                self.mux_sel_0.set_high();
            }
            _ => {
                return None;
            }
        }

        asm::delay(100); // Small delay for the multiplexer to settle.
        Some(self.adc.read(self.adc_pin).unwrap_or(0))
    }

    pub fn read_all(&mut self) -> [u32; 8] {
        let mut values = [0u32; 8];
        for i in 1..=8 {
            values[(i - 1) as usize] = self.read(i).unwrap_or(0);
        }
        values
    }

    pub fn read_all_smoothed(
        &mut self,
        min_freq: f32,
        max_freq_range: f32,
        smoothing_factor: f32,
    ) -> (f32, f32, f32, f32, f32) {
        let knob_vals = self.read_all();

        let target_freq = min_freq + (knob_vals[0] as f32 / 65535.0) * max_freq_range;
        let target_amp = knob_vals[1] as f32 / 65535.0;
        let target_wave_shape = (knob_vals[2] as f32 / 65535.0) * 2.0;
        let target_fold_gain = 1.0 + (knob_vals[3] as f32 / 65535.0) * 9.0;
        let target_pm_amount = (knob_vals[4] as f32 / 65535.0) * 10.0;

        self.smoothed_freq =
            (target_freq * smoothing_factor) + (self.smoothed_freq * (1.0 - smoothing_factor));
        self.smoothed_amp =
            (target_amp * smoothing_factor) + (self.smoothed_amp * (1.0 - smoothing_factor));
        self.smoothed_wave_shape = (target_wave_shape * smoothing_factor)
            + (self.smoothed_wave_shape * (1.0 - smoothing_factor));
        self.smoothed_fold_gain = (target_fold_gain * smoothing_factor)
            + (self.smoothed_fold_gain * (1.0 - smoothing_factor));
        self.smoothed_pm_amount = (target_pm_amount * smoothing_factor)
            + (self.smoothed_pm_amount * (1.0 - smoothing_factor));

        (
            self.smoothed_freq,
            self.smoothed_amp,
            self.smoothed_wave_shape,
            self.smoothed_fold_gain,
            self.smoothed_pm_amount,
        )
    }
}
