// src/hardware.rs

use cortex_m::asm;
use cortex_m::prelude::_embedded_hal_adc_OneShot;
use stm32h7xx_hal::{
    adc,
    gpio::{Analog, Output, Pin, PushPull},
    pac,
};

// I've included the more robust version of this function from our previous conversation
pub fn read_knob_value(
    knob: u8,
    mux_sel_0: &mut Pin<'C', 4, Output<PushPull>>,
    mux_sel_1: &mut Pin<'C', 1, Output<PushPull>>,
    mux_sel_2: &mut Pin<'A', 6, Output<PushPull>>,
    adc: &mut adc::Adc<pac::ADC1, adc::Enabled>,
    adc_pin: &mut Pin<'A', 3, Analog>,
) -> u32 {
    let _ = knob & 0b111; // Ensure index is within 0-7

    match knob {
        1 => {
            mux_sel_2.set_low();
            mux_sel_1.set_low();
            mux_sel_0.set_low();
        }
        2 => {
            mux_sel_2.set_low();
            mux_sel_1.set_high();
            mux_sel_0.set_high();
        }
        _ => {
            // Default to knob 1 if invalid knob number
            mux_sel_2.set_low();
            mux_sel_1.set_low();
            mux_sel_0.set_low();
        }
    }

    asm::delay(100); // Small delay for the multiplexer to settle.
    adc.read(adc_pin).unwrap_or(0)
}
