#![no_main]
#![no_std]

use core::cell::RefCell;
use core::f32::consts::PI;

use cortex_m::asm;
use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;
use libm::sinf;
use panic_halt as _;

use daisy::audio;
use hal::gpio::{Analog, Output, PushPull};
use hal::pac::{self, interrupt};
use stm32h7xx_hal as hal;
use stm32h7xx_hal::{adc, gpio::Pin, prelude::*};

// --- OLED Display Imports ---
use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle},
};
use ssd1306::{prelude::*, Ssd1306};

// --- Constants ---
const SAMPLE_RATE: f32 = 48_000.0;
const TWO_PI: f32 = 2.0 * PI;
const MIN_FREQ: f32 = 20.0;
const MAX_FREQ_RANGE: f32 = 2000.0;
const SMOOTHING_FACTOR: f32 = 0.05;

// --- Oscillator State ---
struct Oscillator {
    phase: f32,
    frequency: f32,
    amplitude: f32,
}

impl Oscillator {
    const fn new() -> Self {
        Self {
            phase: 0.0,
            frequency: 440.0,
            amplitude: 0.5,
        }
    }
    fn set_params(&mut self, frequency: f32, amplitude: f32) {
        self.frequency = frequency;
        self.amplitude = amplitude;
    }
    fn next_sample(&mut self) -> f32 {
        let sample = sinf(self.phase) * self.amplitude;
        self.phase += TWO_PI * self.frequency / SAMPLE_RATE;
        while self.phase >= TWO_PI {
            self.phase -= TWO_PI;
        }
        sample
    }
}

// --- Globals ---
static AUDIO_INTERFACE: Mutex<RefCell<Option<audio::Interface>>> = Mutex::new(RefCell::new(None));
static OSCILLATOR: Mutex<RefCell<Oscillator>> = Mutex::new(RefCell::new(Oscillator::new()));

// --- Audio Callback ---
fn audio_callback(buffer: &mut [(f32, f32); 32]) {
    cortex_m::interrupt::free(|cs| {
        let mut oscillator = OSCILLATOR.borrow(cs).borrow_mut();
        for frame in buffer.iter_mut() {
            let sample = oscillator.next_sample();
            *frame = (sample, sample);
        }
    });
}

// --- Helper Functions ---
fn read_knob(
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

#[entry]
fn main() -> ! {
    // --- Core and Board Setup ---
    let board = daisy::Board::take().unwrap();
    let dp = daisy::pac::Peripherals::take().unwrap();
    let mut cp = cortex_m::Peripherals::take().unwrap();

    cp.SCB.enable_icache();
    cp.SCB.enable_dcache(&mut cp.CPUID);

    let ccdr = daisy::board_freeze_clocks!(board, dp);
    let pins = daisy::board_split_gpios!(board, ccdr, dp);
    let mut led_user = pins.LED_USER.into_push_pull_output();

    // --- ADC and Multiplexer Setup ---
    let mut delay = cp.SYST.delay(ccdr.clocks);
    let mut adc1 = hal::adc::Adc::adc1(
        dp.ADC1,
        4.MHz(),
        &mut delay,
        ccdr.peripheral.ADC12,
        &ccdr.clocks,
    )
    .enable();
    adc1.set_resolution(hal::adc::Resolution::SixteenBit);
    let mut mux_adc_pin = pins.GPIO.PIN_16.into_analog(); // PC3
    let mut mux_sel_0 = pins.GPIO.PIN_21.into_push_pull_output();
    let mut mux_sel_1 = pins.GPIO.PIN_20.into_push_pull_output();
    let mut mux_sel_2 = pins.GPIO.PIN_19.into_push_pull_output();

    // --- Audio Setup ---
    let audio_interface = daisy::board_split_audio!(ccdr, pins).spawn().unwrap();
    cortex_m::interrupt::free(|cs| AUDIO_INTERFACE.borrow(cs).replace(Some(audio_interface)));
    unsafe {
        pac::NVIC::unmask(interrupt::DMA1_STR1);
    }

    // --- OLED Display Setup ---
    let mut display = {
        // The Daisy Field uses a Daisy Seed, so we use the "seed" pin configuration.
        let sck_pin = pins.GPIO.PIN_8.into_alternate(); // SCK: D10 on Field
        let mosi_pin = pins.GPIO.PIN_10.into_alternate(); // MOSI: D9 on Field
        let mut rst_pin = pins.GPIO.PIN_30.into_push_pull_output(); // RESET: A9 on Field
        let dc_pin = pins.GPIO.PIN_9.into_push_pull_output(); // D/C: D8 on Field
        let cs_pin = pins.GPIO.PIN_7.into_push_pull_output(); // CS: D1 on Field

        let spi = dp.SPI1.spi(
            (sck_pin, hal::spi::NoMiso, mosi_pin), // Pass pins in the required (SCK, MISO, MOSI) tuple
            hal::spi::MODE_0,
            3.MHz(),
            ccdr.peripheral.SPI1,
            &ccdr.clocks,
        );

        let interface = display_interface_spi::SPIInterface::new(spi, dc_pin, cs_pin);
        let mut display = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
            .into_buffered_graphics_mode();

        // Reset and initialize the display
        let mut oled_delay = hal::delay::DelayFromCountDownTimer::new(dp.TIM2.timer(
            100.Hz(),
            ccdr.peripheral.TIM2,
            &ccdr.clocks,
        ));
        display.reset(&mut rst_pin, &mut oled_delay).unwrap();
        display.init().unwrap();
        display
    };

    // --- Main Loop ---
    let mut smoothed_freq = MIN_FREQ;
    let mut smoothed_amp = 0.0;
    let one_second = ccdr.clocks.sys_ck().to_Hz();
    const DISPLAY_WIDTH: i32 = 128;
    const DISPLAY_HEIGHT: i32 = 64;
    const DISPLAY_CENTER_Y: i32 = DISPLAY_HEIGHT / 2;
    const MIN_CYCLES: f32 = 1.0; // Min cycles to show on screen
    const MAX_CYCLES: f32 = 10.0; // Max cycles to show on screen

    loop {
        // --- Read Knobs and Update Audio Parameters (same as before) ---
        // --- Read Knobs and Update Parameters ---
        let knob1_val = read_knob(
            1,
            &mut mux_sel_0,
            &mut mux_sel_1,
            &mut mux_sel_2,
            &mut adc1,
            &mut mux_adc_pin,
        );
        let knob2_val = read_knob(
            2,
            &mut mux_sel_0,
            &mut mux_sel_1,
            &mut mux_sel_2,
            &mut adc1,
            &mut mux_adc_pin,
        );

        let target_freq = MIN_FREQ + (knob1_val as f32 / 65535.0) * MAX_FREQ_RANGE;
        let target_amp = knob2_val as f32 / 65535.0;

        smoothed_freq =
            (target_freq * SMOOTHING_FACTOR) + (smoothed_freq * (1.0 - SMOOTHING_FACTOR));
        smoothed_amp = (target_amp * SMOOTHING_FACTOR) + (smoothed_amp * (1.0 - SMOOTHING_FACTOR));

        cortex_m::interrupt::free(|cs| {
            OSCILLATOR
                .borrow(cs)
                .borrow_mut()
                .set_params(smoothed_freq, smoothed_amp);
        });

        // --- Draw Waveform to Display ---
        display.clear(BinaryColor::Off).unwrap();

        // Determine how many cycles to show based on frequency
        let normalized_freq = (smoothed_freq - MIN_FREQ) / MAX_FREQ_RANGE;
        let cycles_on_screen =
            MIN_CYCLES + normalized_freq.max(0.0).min(1.0) * (MAX_CYCLES - MIN_CYCLES);

        // Draw the wave by connecting 127 small lines
        for x in 0..(DISPLAY_WIDTH - 1) {
            // Calculate phase for the current and next pixel
            let phase1 = (x as f32 / DISPLAY_WIDTH as f32) * TWO_PI * cycles_on_screen;
            let phase2 = ((x + 1) as f32 / DISPLAY_WIDTH as f32) * TWO_PI * cycles_on_screen;

            // Calculate sine value, which is between -1.0 and 1.0
            let sin_val1 = sinf(phase1);
            let sin_val2 = sinf(phase2);

            // Convert the sine value to a Y-coordinate on the screen
            // We scale it by the amplitude and the screen's half-height
            let y1 =
                DISPLAY_CENTER_Y - (sin_val1 * smoothed_amp * (DISPLAY_CENTER_Y - 1) as f32) as i32;
            let y2 =
                DISPLAY_CENTER_Y - (sin_val2 * smoothed_amp * (DISPLAY_CENTER_Y - 1) as f32) as i32;

            // Draw a line from the current point to the next
            Line::new(Point::new(x, y1), Point::new(x + 1, y2))
                .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
                .draw(&mut display)
                .unwrap();
        }

        // Flush the buffer to the display
        display.flush().unwrap();

        led_user.toggle();
        asm::delay(one_second / 30); // ~30 FPS refresh rate
    }
}

// --- Interrupt Handler (no changes) ---
#[interrupt]
fn DMA1_STR1() {
    cortex_m::interrupt::free(|cs| {
        if let Some(audio_interface) = AUDIO_INTERFACE.borrow(cs).borrow_mut().as_mut() {
            audio_interface
                .handle_interrupt_dma1_str1(audio_callback)
                .unwrap();
        }
    });
}
