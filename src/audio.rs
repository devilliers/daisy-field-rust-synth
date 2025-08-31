// src/audio.rs

use daisy::audio::{self, Block};
use rtt_target::{rprintln, rtt_init_print};

pub const BLOCK_LENGTH: usize = audio::BLOCK_LENGTH;

// This is our shared buffer. The audio task writes to it,
// and the audio interrupt reads from it.
static mut BUFFER: [(f32, f32); BLOCK_LENGTH] = [(0.0, 0.0); BLOCK_LENGTH];

pub struct Audio {
    interface: Option<daisy::audio::Interface>,
}

impl Audio {
    pub fn init(interface: daisy::audio::Interface) -> Self {
        Self {
            interface: Some(interface),
        }
    }

    pub fn spawn(&mut self) {
        self.interface = Some(self.interface.take().unwrap().spawn().unwrap());
    }

    // This function will be called from our RTIC audio task to
    // prepare the next block of audio.
    pub fn update_buffer(&mut self, mut callback: impl FnMut(&mut [(f32, f32); BLOCK_LENGTH])) {
        let buffer: &'static mut [(f32, f32); BLOCK_LENGTH] = unsafe { &mut BUFFER };
        callback(buffer);
        self.interface
            .as_mut()
            .unwrap()
            .handle_interrupt_dma1_str1(audio_callback)
            .unwrap();
    }
}

// This is the permanent audio callback.
fn audio_callback(block: &mut Block) {
    let buffer: &'static mut [(f32, f32); BLOCK_LENGTH] = unsafe { &mut BUFFER };
    for (source, target) in buffer.iter().zip(block.iter_mut()) {
        *target = *source;
    }
}
