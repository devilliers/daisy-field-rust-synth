// src/main.rs

#![no_main]
#![no_std]

// Declare the modules
mod audio;
mod display;
mod hardware;
mod oscillator;

use cortex_m::asm;
use cortex_m_rt::entry;
use panic_halt as _;

use stm32h7xx_hal as hal;

use panic_halt as _;

use ssd1306::mode::DisplayConfig;
use stm32h7xx_hal::prelude::*;

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
    cortex_m::interrupt::free(|cs| {
        audio::AUDIO_INTERFACE
            .borrow(cs)
            .replace(Some(audio_interface))
    });
    unsafe {
        hal::pac::NVIC::unmask(hal::pac::interrupt::DMA1_STR1);
    }

    // --- OLED Display Setup ---
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
    let one_second = ccdr.clocks.sys_ck().to_Hz();

    loop {
        // --- Read Knobs and Update Parameters ---
        let knob1_val = hardware::read_knob_value(
            0,
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

        let target_freq = MIN_FREQ + (knob1_val as f32 / 65535.0) * MAX_FREQ_RANGE;
        let target_amp = knob2_val as f32 / 65535.0;

        smoothed_freq =
            (target_freq * SMOOTHING_FACTOR) + (smoothed_freq * (1.0 - SMOOTHING_FACTOR));
        smoothed_amp = (target_amp * SMOOTHING_FACTOR) + (smoothed_amp * (1.0 - SMOOTHING_FACTOR));

        cortex_m::interrupt::free(|cs| {
            audio::OSCILLATOR
                .borrow(cs)
                .borrow_mut()
                .set_params(smoothed_freq, smoothed_amp);
        });

        // --- Draw Waveform to Display ---
        display::draw_waveform(
            &mut display,
            smoothed_freq,
            smoothed_amp,
            MIN_FREQ,
            MAX_FREQ_RANGE,
        );

        led_user.toggle();
        asm::delay(one_second / 100);
    }
}
