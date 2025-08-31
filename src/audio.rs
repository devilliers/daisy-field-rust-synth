// src/audio.rs

use crate::{oscillator::Oscillator, recording}; // Add recording
use core::cell::RefCell;
use cortex_m::interrupt::{free, Mutex};
use daisy::audio;
use stm32h7xx_hal::interrupt;

// --- Globals ---
pub static AUDIO_INTERFACE: Mutex<RefCell<Option<audio::Interface>>> =
    Mutex::new(RefCell::new(None));
pub static OSCILLATOR: Mutex<RefCell<Oscillator>> = Mutex::new(RefCell::new(Oscillator::new()));

// --- Audio Callback ---
fn audio_callback(buffer: &mut [(f32, f32); 32]) {
    free(|cs| {
        let mut oscillator = OSCILLATOR.borrow(cs).borrow_mut();
        let mut recorder = recording::RECORDER.borrow(cs).borrow_mut();

        for frame in buffer.iter_mut() {
            let sample = oscillator.next_sample();
            *frame = (sample, sample);
            // Record the left channel sample
            recorder.record_sample(sample);
        }
    });
}

// --- Interrupt Handler ---
#[interrupt]
fn DMA1_STR1() {
    free(|cs| {
        if let Some(audio_interface) = AUDIO_INTERFACE.borrow(cs).borrow_mut().as_mut() {
            audio_interface
                .handle_interrupt_dma1_str1(audio_callback)
                .unwrap();
        }
    });
}
