// src/main.rs

#![no_main]
#![no_std]

// Declare the modules
mod audio;
mod display;
mod hardware;
mod oscillator;
mod recording;

use cortex_m::asm;
use cortex_m_rt::entry;
use panic_halt as _;

use ssd1306::mode::DisplayConfig;
use stm32h7xx_hal as hal;

// --- Corrected and Organized Imports ---
use hal::{
    // We need the ADC's Resolution enum for the DAC
    adc::Resolution as AdcResolution,
    // Import the concrete Channel1 and Enabled types for the DAC.
    // Channel1 is the type-erased wrapper for the specific pin.
    dac::{Enabled, C1},
    device::DAC,
    gpio::Speed,
    // This prelude brings in essential traits like DacExt for the .split() method
    prelude::*,
    sdmmc::{SdCard, Sdmmc},
};

use core::cell::RefCell;
use cortex_m::interrupt::Mutex;

// --- Use the concrete, Sized, and Send `Channel1<Enabled>` struct type ---
pub static DAC1: Mutex<RefCell<Option<C1<DAC, Enabled>>>> = Mutex::new(RefCell::new(None));

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

    // --- Button Setup (Using SW2 on Pin 29 to avoid conflict) ---
    let sw1 = pins.GPIO.PIN_29.into_pull_up_input();

    // --- SD Card Setup (Corrected Pins based on Schematic) ---
    let clk = pins.GPIO.PIN_6.into_alternate().speed(Speed::VeryHigh);
    let cmd = pins
        .GPIO
        .PIN_5
        .into_alternate()
        .internal_pull_up(true)
        .speed(Speed::VeryHigh);
    let d0 = pins
        .GPIO
        .PIN_4
        .into_alternate()
        .internal_pull_up(true)
        .speed(Speed::VeryHigh);
    let d1 = pins
        .GPIO
        .PIN_3
        .into_alternate()
        .internal_pull_up(true)
        .speed(Speed::VeryHigh);
    let d2 = pins
        .GPIO
        .PIN_2
        .into_alternate()
        .internal_pull_up(true)
        .speed(Speed::VeryHigh);
    let d3 = pins
        .GPIO
        .PIN_1
        .into_alternate()
        .internal_pull_up(true)
        .speed(Speed::VeryHigh);

    let mut sdmmc: Sdmmc<_, SdCard> = dp.SDMMC1.sdmmc(
        (clk, cmd, d0, d1, d2, d3),
        ccdr.peripheral.SDMMC1,
        &ccdr.clocks,
    );

    // Initialize card at a lower speed
    while sdmmc.init(10.MHz()).is_err() {
        led_user.toggle();
        asm::delay(ccdr.clocks.sys_ck().to_Hz() / 20); // Blink fast
    }

    // --- Correct DAC Setup using .split() ---
    let dac1_pin = pins.GPIO.PIN_23.into_analog(); // A8/D23 is DAC_OUT_1
    let dac2_pin = pins.GPIO.PIN_22.into_analog(); // A7/D22 is DAC_OUT_2

    // The DacExt trait (brought in by the prelude) provides the .split() method
    let (mut dac1, _dac2) = dp
        .DAC
        .split(ccdr.peripheral.DAC, &ccdr.clocks, dac1_pin, dac2_pin);

    dac1.set_resolution(AdcResolution::TwelveBit);
    let dac1_out = dac1.enable();

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
    adc1.set_resolution(AdcResolution::SixteenBit);
    let mut mux_adc_pin = pins.GPIO.PIN_16.into_analog();
    let mut mux_sel_0 = pins.GPIO.PIN_21.into_push_pull_output();
    let mut mux_sel_1 = pins.GPIO.PIN_20.into_push_pull_output();
    let mut mux_sel_2 = pins.GPIO.PIN_19.into_push_pull_output();

    // --- Audio Setup ---
    let audio_interface = daisy::board_split_audio!(ccdr, pins).spawn().unwrap();
    cortex_m::interrupt::free(|cs| {
        audio::AUDIO_INTERFACE
            .borrow(cs)
            .replace(Some(audio_interface));
        // Store the initialized DAC in the global static variable
        DAC1.borrow(cs).replace(Some(dac1_out));
    });
    unsafe {
        hal::pac::NVIC::unmask(hal::pac::interrupt::DMA1_STR1);
    }

    // --- OLED Display Setup (Uses Pin 30 for Reset) ---
    let mut display = {
        let sck = pins.GPIO.PIN_8.into_alternate();
        let mosi = pins.GPIO.PIN_10.into_alternate();
        let mut rst = pins.GPIO.PIN_30.into_push_pull_output();
        let dc = pins.GPIO.PIN_9.into_push_pull_output();
        let cs = pins.GPIO.PIN_7.into_push_pull_output();

        let spi = dp.SPI1.spi(
            (sck, hal::spi::NoMiso, mosi),
            hal::spi::MODE_0,
            3.MHz(),
            ccdr.peripheral.SPI1,
            &ccdr.clocks,
        );
        let interface = display_interface_spi::SPIInterface::new(spi, dc, cs);
        let mut display = ssd1306::Ssd1306::new(
            interface,
            ssd1306::prelude::DisplaySize128x64,
            ssd1306::prelude::DisplayRotation::Rotate0,
        )
        .into_buffered_graphics_mode();

        let mut oled_delay = hal::delay::DelayFromCountDownTimer::new(dp.TIM2.timer(
            100.Hz(),
            ccdr.peripheral.TIM2,
            &ccdr.clocks,
        ));
        display.reset(&mut rst, &mut oled_delay).unwrap();
        display.init().unwrap();
        display
    };

    // --- Main Loop ---
    const MIN_FREQ: f32 = 20.0;
    const MAX_FREQ_RANGE: f32 = 6000.0;
    const SMOOTHING_FACTOR: f32 = 0.50;
    let mut smoothed_freq = MIN_FREQ;
    let mut smoothed_amp = 0.0;
    let mut smoothed_wave_shape = 0.0;
    let mut smoothed_fold_gain = 1.0;
    let mut smoothed_pm_amount = 0.0;
    let one_second = ccdr.clocks.sys_ck().to_Hz();

    let mut last_sw1_state = sw1.is_high();
    let mut next_block_to_write: u32 = 0;

    loop {
        // --- Check for Recording Trigger ---
        let current_sw1_state = sw1.is_high();
        if last_sw1_state && !current_sw1_state {
            cortex_m::interrupt::free(|cs| {
                let mut recorder = recording::RECORDER.borrow(cs).borrow_mut();
                recorder.toggle_recording();
                // When starting a new recording, reset the block counter
                if recorder.is_recording {
                    next_block_to_write = 0;
                }
            });
        }
        last_sw1_state = current_sw1_state;

        // --- Check if a buffer needs to be written to the SD card ---
        let buffer_to_write = cortex_m::interrupt::free(|cs| {
            recording::RECORDER
                .borrow(cs)
                .borrow_mut()
                .write_buffer
                .take()
        });

        if let Some(buffer) = buffer_to_write {
            led_user.set_high();
            let data_to_write = cortex_m::interrupt::free(|cs| {
                let recorder = recording::RECORDER.borrow(cs).borrow();
                match buffer {
                    recording::Buffer::Ping => recorder.ping_buffer,
                    recording::Buffer::Pong => recorder.pong_buffer,
                }
            });

            // Convert i16 slice to u8 slice for writing
            let byte_slice: &[u8] =
                unsafe { core::slice::from_raw_parts(data_to_write.as_ptr() as *const u8, 512) };

            // Convert the slice to a fixed-size array reference before writing
            let block_to_write: &[u8; 512] = byte_slice.try_into().unwrap();

            // Write the raw block to the card
            if sdmmc
                .write_block(next_block_to_write, block_to_write)
                .is_ok()
            {
                next_block_to_write += 1;
            }
            led_user.set_low();
        }

        // --- Read Knobs and Update Parameters ---
        let knob1_val = hardware::read_knob_value(
            1,
            &mut mux_sel_0,
            &mut mux_sel_1,
            &mut mux_sel_2,
            &mut adc1,
            &mut mux_adc_pin,
        );
        let knob2_val = hardware::read_knob_value(
            2,
            &mut mux_sel_0,
            &mut mux_sel_1,
            &mut mux_sel_2,
            &mut adc1,
            &mut mux_adc_pin,
        );
        let knob3_val = hardware::read_knob_value(
            3,
            &mut mux_sel_0,
            &mut mux_sel_1,
            &mut mux_sel_2,
            &mut adc1,
            &mut mux_adc_pin,
        );
        let knob4_val = hardware::read_knob_value(
            4,
            &mut mux_sel_0,
            &mut mux_sel_1,
            &mut mux_sel_2,
            &mut adc1,
            &mut mux_adc_pin,
        );
        let knob5_val = hardware::read_knob_value(
            5,
            &mut mux_sel_0,
            &mut mux_sel_1,
            &mut mux_sel_2,
            &mut adc1,
            &mut mux_adc_pin,
        );

        let target_freq = MIN_FREQ + (knob1_val.unwrap() as f32 / 65535.0) * MAX_FREQ_RANGE;
        let target_amp = knob2_val.unwrap() as f32 / 65535.0;
        let target_wave_shape = (knob3_val.unwrap() as f32 / 65535.0) * 2.0;
        let target_fold_gain = 1.0 + (knob4_val.unwrap() as f32 / 65535.0) * 9.0;
        let target_pm_amount = (knob5_val.unwrap() as f32 / 65535.0) * 10.0;

        smoothed_freq =
            (target_freq * SMOOTHING_FACTOR) + (smoothed_freq * (1.0 - SMOOTHING_FACTOR));
        smoothed_amp = (target_amp * SMOOTHING_FACTOR) + (smoothed_amp * (1.0 - SMOOTHING_FACTOR));
        smoothed_wave_shape = (target_wave_shape * SMOOTHING_FACTOR)
            + (smoothed_wave_shape * (1.0 - SMOOTHING_FACTOR));
        smoothed_fold_gain =
            (target_fold_gain * SMOOTHING_FACTOR) + (smoothed_fold_gain * (1.0 - SMOOTHING_FACTOR));
        smoothed_pm_amount =
            (target_pm_amount * SMOOTHING_FACTOR) + (smoothed_pm_amount * (1.0 - SMOOTHING_FACTOR));

        cortex_m::interrupt::free(|cs| {
            audio::OSCILLATOR.borrow(cs).borrow_mut().set_params(
                smoothed_freq,
                smoothed_amp,
                smoothed_wave_shape,
                smoothed_fold_gain,
                smoothed_pm_amount,
            );
        });

        // --- Draw Waveform to Display ---
        display::draw_waveform(
            &mut display,
            smoothed_freq,
            smoothed_amp,
            MIN_FREQ,
            MAX_FREQ_RANGE,
            smoothed_wave_shape,
            smoothed_fold_gain,
            smoothed_pm_amount,
        );

        asm::delay(one_second / 200);
    }
}
