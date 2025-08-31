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
) -> Option<u32> {
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
        3 => {
            mux_sel_2.set_low();
            mux_sel_1.set_low();
            mux_sel_0.set_high();
        }
        4 => {
            mux_sel_2.set_high();
            mux_sel_1.set_low();
            mux_sel_0.set_low();
        }
        5 => {
            mux_sel_2.set_low();
            mux_sel_1.set_high();
            mux_sel_0.set_low();
        }
        6 => {
            mux_sel_2.set_high();
            mux_sel_1.set_low();
            mux_sel_0.set_high();
        }
        7 => {
            mux_sel_2.set_high();
            mux_sel_1.set_high();
            mux_sel_0.set_low();
        }
        8 => {
            mux_sel_2.set_high();
            mux_sel_1.set_high();
            mux_sel_0.set_high();
        }
        _ => {
            return None;
        }
    }

    asm::delay(100); // Small delay for the multiplexer to settle.
    Some(adc.read(adc_pin).unwrap_or(0))
}
