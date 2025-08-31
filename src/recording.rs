// src/recording.rs

use core::cell::RefCell;
use cortex_m::interrupt::Mutex;

// 256 samples * 2 bytes/sample = 512 bytes, the size of one SD card block.
const BUFFER_SIZE: usize = 256;

#[derive(Clone, Copy)]
pub enum Buffer {
    Ping,
    Pong,
}

pub struct RecordingState {
    pub is_recording: bool,
    pub ping_buffer: [i16; BUFFER_SIZE],
    pub pong_buffer: [i16; BUFFER_SIZE],
    pub write_buffer: Option<Buffer>,
    buffer_idx: usize,
    current_buffer: Buffer,
}

impl RecordingState {
    pub const fn new() -> Self {
        Self {
            is_recording: false,
            ping_buffer: [0; BUFFER_SIZE],
            pong_buffer: [0; BUFFER_SIZE],
            write_buffer: None,
            buffer_idx: 0,
            current_buffer: Buffer::Ping,
        }
    }

    pub fn toggle_recording(&mut self) {
        self.is_recording = !self.is_recording;
        self.buffer_idx = 0;
    }

    // This method is called from the high-priority audio callback
    pub fn record_sample(&mut self, sample: f32) {
        if !self.is_recording || self.write_buffer.is_some() {
            return;
        }

        let scaled_sample = (sample * 32767.0) as i16;

        match self.current_buffer {
            Buffer::Ping => self.ping_buffer[self.buffer_idx] = scaled_sample,
            Buffer::Pong => self.pong_buffer[self.buffer_idx] = scaled_sample,
        }

        self.buffer_idx += 1;

        if self.buffer_idx >= BUFFER_SIZE {
            self.buffer_idx = 0;
            // Swap buffers and signal to main loop
            match self.current_buffer {
                Buffer::Ping => {
                    self.write_buffer = Some(Buffer::Ping);
                    self.current_buffer = Buffer::Pong;
                }
                Buffer::Pong => {
                    self.write_buffer = Some(Buffer::Pong);
                    self.current_buffer = Buffer::Ping;
                }
            }
        }
    }
}

pub static RECORDER: Mutex<RefCell<RecordingState>> =
    Mutex::new(RefCell::new(RecordingState::new()));
