// src/main.rs

#![no_main]
#![no_std]

mod audio;
mod display;
mod hardware;
mod oscillator;

use panic_halt as _;
use rtic::app;
use rtt_target::{rprintln, rtt_init_print};
use systick_monotonic::fugit::ExtU64;

use systick_monotonic::Systick;

#[app(device = daisy::pac, peripherals = true, dispatchers = [SPI1])]
mod app {
    use super::*;

    use crate::audio::Audio;
    use crate::display::draw_waveform;
    use crate::hardware::Knobs;
    use crate::oscillator::Oscillator;
    use stm32h7xx_hal::gpio::{Output, Pin, PushPull};

    use daisy::hal;
    use hal::prelude::*;
    use ssd1306::{mode::BufferedGraphicsMode, prelude::*, Ssd1306};

    pub type OledDisplay = Ssd1306<
        display_interface_spi::SPIInterface<
            daisy::hal::spi::Spi<daisy::pac::SPI1, hal::spi::Enabled>,
            Pin<'B', 4, Output<PushPull>>,
            Pin<'G', 10, Output<PushPull>>,
        >,
        DisplaySize128x64,
        BufferedGraphicsMode<DisplaySize128x64>,
    >;

    #[shared]
    struct Shared {
        oscillator: Oscillator,
    }

    #[local]
    struct Local {
        display: OledDisplay,
        knobs: Knobs<'static>,
        audio: Audio,
    }

    // Bind the SysTick interrupt and set the tick rate.
    #[monotonic(binds = SysTick, default = true)]
    type MonoTimer = Systick<1000>;

    #[init(local = [
        _mux_sel_0: Option<hal::gpio::gpioc::PC4<hal::gpio::Output<hal::gpio::PushPull>>> = None,
        _mux_sel_1: Option<hal::gpio::gpioc::PC1<hal::gpio::Output<hal::gpio::PushPull>>> = None,
        _mux_sel_2: Option<hal::gpio::gpioa::PA6<hal::gpio::Output<hal::gpio::PushPull>>> = None,
        _adc1: Option<hal::adc::Adc<hal::pac::ADC1, hal::adc::Enabled>> = None,
        _mux_adc_pin: Option<hal::gpio::gpioa::PA3<hal::gpio::Analog>> = None,
    ])]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        rtt_init_print!();
        rprintln!("init");

        let dp = cx.device;
        let board = daisy::Board::take().unwrap();
        let ccdr = daisy::board_freeze_clocks!(board, dp);
        let pins = daisy::board_split_gpios!(board, ccdr, dp);

        let mut cp = cx.core;
        cp.SCB.enable_icache();
        cp.SCB.enable_dcache(&mut cp.CPUID);

        // Initialize the monotonic timer here using the system clock frequency
        let mono = MonoTimer::new(cp.SYST, ccdr.clocks.sys_ck().to_Hz());

        let mut delay = hal::delay::DelayFromCountDownTimer::new(dp.TIM2.timer(
            100.Hz(),
            ccdr.peripheral.TIM2,
            &ccdr.clocks,
        ));
        let mut adc1 = hal::adc::Adc::adc1(
            dp.ADC1,
            4.MHz(),
            &mut delay,
            ccdr.peripheral.ADC12,
            &ccdr.clocks,
        )
        .enable();
        adc1.set_resolution(hal::adc::Resolution::SixteenBit);
        let mux_sel_0 = cx
            .local
            ._mux_sel_0
            .insert(pins.GPIO.PIN_21.into_push_pull_output());
        let mux_sel_1 = cx
            .local
            ._mux_sel_1
            .insert(pins.GPIO.PIN_20.into_push_pull_output());
        let mux_sel_2 = cx
            .local
            ._mux_sel_2
            .insert(pins.GPIO.PIN_19.into_push_pull_output());
        let adc1_static = cx.local._adc1.insert(adc1);
        let mux_adc_pin = cx.local._mux_adc_pin.insert(pins.GPIO.PIN_16.into_analog());
        let knobs = Knobs::new(mux_sel_0, mux_sel_1, mux_sel_2, adc1_static, mux_adc_pin);

        let mut audio = Audio::init(daisy::board_split_audio!(ccdr, pins));
        audio.spawn();

        unsafe {
            hal::pac::NVIC::unmask(hal::pac::interrupt::DMA1_STR1);
        }

        let display: OledDisplay = {
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
            let mut display = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
                .into_buffered_graphics_mode();
            display.reset(&mut rst, &mut delay).unwrap();
            display.init().unwrap();
            display
        };

        ui_update::spawn().unwrap();

        (
            Shared {
                oscillator: Oscillator::new(),
            },
            Local {
                display,
                knobs,
                audio,
            },
            init::Monotonics(mono),
        )
    }

    #[task(binds = DMA1_STR1, local = [audio], shared = [oscillator], priority = 2)]
    fn audio(mut cx: audio::Context) {
        let audio = cx.local.audio;
        audio.update_buffer(|buffer| {
            cx.shared.oscillator.lock(|osc| {
                for frame in buffer.iter_mut() {
                    let sample = osc.next_sample();
                    *frame = (sample, sample);
                }
            });
        });
    }

    #[task(local = [display, knobs], shared = [oscillator], priority = 1, capacity = 1)]
    fn ui_update(mut cx: ui_update::Context) {
        const MIN_FREQ: f32 = 20.0;
        const MAX_FREQ_RANGE: f32 = 3000.0;
        const SMOOTHING_FACTOR: f32 = 1.0;

        let (
            smoothed_freq,
            smoothed_amp,
            smoothed_wave_shape,
            smoothed_fold_gain,
            smoothed_pm_amount,
        ) = cx
            .local
            .knobs
            .read_all_smoothed(MIN_FREQ, MAX_FREQ_RANGE, SMOOTHING_FACTOR);
        cx.shared.oscillator.lock(|osc| {
            osc.set_params(
                smoothed_freq,
                smoothed_amp,
                smoothed_wave_shape,
                smoothed_fold_gain,
                smoothed_pm_amount,
            );
        });
        draw_waveform(
            cx.local.display,
            smoothed_freq,
            smoothed_amp,
            MIN_FREQ,
            MAX_FREQ_RANGE,
            smoothed_wave_shape,
            smoothed_fold_gain,
            smoothed_pm_amount,
        );

        ui_update::spawn_after(33u64.millis()).unwrap();
    }
}
