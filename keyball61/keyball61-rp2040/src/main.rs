#![no_std]
#![no_main]

use core::panic::PanicInfo;

use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_rp::{
    bind_interrupts,
    gpio::{Input, Level, Output},
    i2c::I2c,
    peripherals::{I2C1, PIO0, PIO1, USB},
    pio::Pio,
};

use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use rktk::{drivers::Drivers, hooks::create_empty_hooks, none_driver};
use rktk_drivers_common::{
    display::ssd1306::{prelude::DisplaySize128x32, Ssd1306DisplayBuilder},
    keyscan::{detect_hand_from_matrix, duplex_matrix::DuplexMatrixScanner},
    mouse::pmw3360::Pmw3360Builder,
    panic_utils,
    usb::{CommonUsbDriverBuilder, UsbOpts},
};
use rktk_drivers_rp::{
    keyscan::flex_pin::RpFlexPin, mouse::pmw3360, rgb::ws2812_pio::Ws2812Pio,
    split::pio_half_duplex::PioHalfDuplexSplitDriver,
};

use keyball_common::*;

bind_interrupts!(pub struct Irqs {
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<I2C1>;
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
    PIO1_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO1>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut cfg = embassy_rp::config::Config::default();
    cfg.clocks.sys_clk.div_int = 2;
    let mut p = embassy_rp::init(cfg);

    let display = Ssd1306DisplayBuilder::new(
        I2c::new_async(
            p.I2C1,
            p.PIN_3,
            p.PIN_2,
            Irqs,
            rktk_drivers_rp::display::ssd1306::recommended_i2c_config(),
        ),
        DisplaySize128x32,
    );

    let Some(display) = panic_utils::display_message_if_panicked(display).await else {
        cortex_m::asm::udf()
    };

    let spi = Mutex::<NoopRawMutex, _>::new(embassy_rp::spi::Spi::new(
        p.SPI0,
        p.PIN_22,
        p.PIN_23,
        p.PIN_20,
        p.DMA_CH0,
        p.DMA_CH1,
        pmw3360::recommended_spi_config(),
    ));
    let ball_spi = SpiDevice::new(&spi, Output::new(p.PIN_21, embassy_rp::gpio::Level::High));
    let ball = Pmw3360Builder::new(ball_spi);

    let hand = detect_hand_from_matrix(
        Output::new(&mut p.PIN_6, Level::Low),
        Input::new(&mut p.PIN_26, embassy_rp::gpio::Pull::Down),
        None,
        None,
    )
    .await
    .unwrap();
    let keyscan = DuplexMatrixScanner::<_, _, 5, 4, 5, 7>::new(
        [
            RpFlexPin::new(p.PIN_4),
            RpFlexPin::new(p.PIN_5),
            RpFlexPin::new(p.PIN_6),
            RpFlexPin::new(p.PIN_7),
            RpFlexPin::new(p.PIN_8),
        ],
        [
            RpFlexPin::new(p.PIN_29),
            RpFlexPin::new(p.PIN_28),
            RpFlexPin::new(p.PIN_27),
            RpFlexPin::new(p.PIN_26),
        ],
        Some(rktk_drivers_common::keyscan::duplex_matrix::OutputWait::Pin),
        translate_key_position(hand),
    );

    let usb = {
        let driver = embassy_rp::usb::Driver::new(p.USB, Irqs);
        let usb_opts = UsbOpts {
            config: USB_CONFIG,
            mouse_poll_interval: 5,
            kb_poll_interval: 5,
            driver,
            defmt_usb_use_dtr: true,
        };

        CommonUsbDriverBuilder::new(usb_opts)
    };

    let pio = Pio::new(p.PIO0, Irqs);
    let split = PioHalfDuplexSplitDriver::new(pio, p.PIN_1);

    let pio = Pio::new(p.PIO1, Irqs);
    let rgb = Ws2812Pio::new(pio, p.PIN_0, p.DMA_CH2);

    // NOTE: needed for some macro thing. maybe this can be avoided.
    #[allow(clippy::needless_late_init)]
    let storage;
    rktk_drivers_rp::init_storage!(storage, p.FLASH, p.DMA_CH3, { 4 * 1024 * 1024 });

    let drivers = Drivers {
        keyscan,
        system: rktk_drivers_rp::system::RpSystemDriver,
        mouse_builder: Some(ball),
        usb_builder: Some(usb),
        display_builder: Some(display),
        split: Some(split),
        rgb: Some(rgb),
        ble_builder: none_driver!(BleBuilder),
        storage: Some(storage),
        debounce: none_driver!(Debounce),
        encoder: none_driver!(Encoder),
    };

    rktk::task::start(drivers, &keymap::KEYMAP, Some(hand), create_empty_hooks()).await;
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    cortex_m::interrupt::disable();
    panic_utils::save_panic_info(info);
    cortex_m::peripheral::SCB::sys_reset()
}
