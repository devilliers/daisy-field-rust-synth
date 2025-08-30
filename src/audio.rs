// src/audio.rs

use crate::oscillator::Oscillator;
use core::cell::RefCell;
use cortex_m::interrupt::{free, Mutex}; // Only import `free` and `Mutex`
use daisy::audio;
use stm32h7xx_hal::interrupt;

// --- Globals ---
pub static AUDIO_INTERFACE: Mutex<RefCell<Option<audio::Interface>>> =
    Mutex::new(RefCell::new(None));
pub static OSCILLATOR: Mutex<RefCell<Oscillator>> = Mutex::new(RefCell::new(Oscillator::new()));

// --- Audio Callback ---
fn audio_callback(buffer: &mut [(f32, f32); 32]) {
    // --- AND CHANGE THIS LINE ---
    free(|cs| {
        // Use `free` directly instead of `interrupt::free`
        let mut oscillator = OSCILLATOR.borrow(cs).borrow_mut();
        for frame in buffer.iter_mut() {
            let sample = oscillator.next_sample();
            *frame = (sample, sample);
        }
    });
}

// --- Interrupt Handler ---
#[interrupt]
fn DMA1_STR1() {
    // --- AND CHANGE THIS LINE ---
    free(|cs| {
        // Use `free` directly instead of `interrupt::free`
        if let Some(audio_interface) = AUDIO_INTERFACE.borrow(cs).borrow_mut().as_mut() {
            audio_interface
                .handle_interrupt_dma1_str1(audio_callback)
                .unwrap();
        }
    });
}
