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
use hal::pac::{self, interrupt};
use stm32h7xx_hal as hal;
use stm32h7xx_hal::{adc, gpio::GpioExt, prelude::*};

// --- Globals ---
static AUDIO_INTERFACE: Mutex<RefCell<Option<audio::Interface>>> = Mutex::new(RefCell::new(None));
static mut PHASE: f32 = 0.0;
static mut FREQUENCY: f32 = 440.0;
static mut AMPLITUDE: f32 = 0.5;
const SAMPLE_RATE: f32 = 48000.0;

fn audio_callback(buffer: &mut [(f32, f32); 32]) {
    for frame in buffer.iter_mut() {
        unsafe {
            let sample = sinf(PHASE) * AMPLITUDE;
            *frame = (sample, sample);

            PHASE += 2.0 * PI * FREQUENCY / SAMPLE_RATE;
            if PHASE > 2.0 * PI {
                PHASE -= 2.0 * PI;
            }
        }
    }
}

#[entry]
fn main() -> ! {
    let mut cp = cortex_m::Peripherals::take().unwrap();
    let dp = pac::Peripherals::take().unwrap();

    cp.SCB.enable_icache();
    cp.SCB.enable_dcache(&mut cp.CPUID);

    let board = daisy::Board::take().unwrap();
    let ccdr = daisy::board_freeze_clocks!(board, dp);

    // --- ADC Setup for Knobs ---
    let mut delay = cp.SYST.delay(ccdr.clocks);
    let mut adc1 = adc::Adc::adc1(
        dp.ADC1,
        4.MHz(),
        &mut delay,
        ccdr.peripheral.ADC12,
        &ccdr.clocks,
    )
    .enable();

    adc1.set_resolution(adc::Resolution::SixteenBit);
    adc1.set_sample_time(adc::AdcSampleTime::T_810);

    // --- Manually expand the `board_split_gpios!` macro ---
    let pins = board.split_gpios(
        dp.GPIOA.split(ccdr.peripheral.GPIOA),
        dp.GPIOB.split(ccdr.peripheral.GPIOB),
        dp.GPIOC.split(ccdr.peripheral.GPIOC),
        dp.GPIOD.split(ccdr.peripheral.GPIOD),
        dp.GPIOE.split(ccdr.peripheral.GPIOE),
        dp.GPIOF.split(ccdr.peripheral.GPIOF),
        dp.GPIOG.split(ccdr.peripheral.GPIOG),
        dp.GPIOH.split(ccdr.peripheral.GPIOH),
        dp.GPIOI.split(ccdr.peripheral.GPIOI),
    );

    let mut led_user = daisy::board_split_leds!(pins).USER;

    // --- Setup Multiplexer Pins ---
    // The single ADC pin for all knobs
    let mut mux_adc_pin = pins.GPIO.PIN_16.into_analog();
    // The 3 selector pins, configured as outputs
    let mut mux_sel_0 = pins.GPIO.PIN_26.into_push_pull_output();
    let mut mux_sel_1 = pins.GPIO.PIN_27.into_push_pull_output();
    let mut mux_sel_2 = pins.GPIO.PIN_28.into_push_pull_output();

    // --- Audio Setup ---
    let audio_interface = daisy::board_split_audio!(ccdr, pins).spawn().unwrap();
    cortex_m::interrupt::free(|cs| {
        AUDIO_INTERFACE.borrow(cs).replace(Some(audio_interface));
    });
    unsafe {
        pac::NVIC::unmask(interrupt::DMA1_STR1);
    }

    // --- Smoothing Variables ---
    let mut smoothed_freq = 20.0;
    let mut smoothed_amp = 0.0;
    let smoothing_factor = 0.05;

    let one_second = ccdr.clocks.sys_ck().to_Hz();
    loop {
        // --- Read Knob 1 (Channel 4 -> 100) ---
        mux_sel_2.set_high();
        mux_sel_1.set_low();
        mux_sel_0.set_low();
        asm::delay(100); // Small delay for MUX to settle
        let knob1_val: u32 = adc1.read(&mut mux_adc_pin).unwrap();

        // --- Read Knob 2 (Channel 5 -> 101) ---
        mux_sel_2.set_high();
        mux_sel_1.set_low();
        mux_sel_0.set_high();
        asm::delay(100); // Small delay for MUX to settle
        let knob2_val: u32 = adc1.read(&mut mux_adc_pin).unwrap();

        // Map knob values to frequency and amplitude
        let freq_hz = 20.0 + (knob1_val as f32 / 65535.0) * 2000.0;
        let amp = knob2_val as f32 / 65535.0;

        // --- Apply Smoothing ---
        smoothed_freq = (freq_hz * smoothing_factor) + (smoothed_freq * (1.0 - smoothing_factor));
        smoothed_amp = (amp * smoothing_factor) + (smoothed_amp * (1.0 - smoothing_factor));

        unsafe {
            FREQUENCY = smoothed_freq;
            AMPLITUDE = smoothed_amp;
        }

        led_user.toggle();
        asm::delay(one_second / 100);
    }
}

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
