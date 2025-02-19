#![no_std]
#![no_main]

use core::panic::PanicInfo;

use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_nrf::{
    gpio::{Output, Pin},
    interrupt::{self, InterruptExt, Priority},
    peripherals::SPI2,
    ppi::Group,
    spim::Spim,
    twim::Twim,
    usb::vbus_detect::SoftwareVbusDetect,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use once_cell::sync::OnceCell;

use rktk::{drivers::Drivers, hooks::create_empty_hooks, none_driver};
use rktk_drivers_common::{
    debounce::EagerDebounceDriver,
    display::ssd1306::Ssd1306DisplayBuilder,
    keyscan::{duplex_matrix::DuplexMatrixScanner, HandDetector},
    mouse::pmw3360::Pmw3360Builder,
    panic_utils,
};
use rktk_drivers_nrf::{
    keyscan::flex_pin::NrfFlexPin, rgb::ws2812_pwm::Ws2812Pwm, softdevice::flash::get_flash,
    split::uart_half_duplex::UartHalfDuplexSplitDriver, system::NrfSystemDriver,
};

use keyball_common::*;

use nrf_softdevice as _;

#[cfg(feature = "ble")]
mod ble {
    pub use rktk_drivers_nrf::softdevice::ble::init_ble_server;
    pub use rktk_drivers_nrf::softdevice::ble::NrfBleDriverBuilder;
}

#[cfg(feature = "usb")]
mod usb {
    pub use rktk_drivers_common::usb::CommonUsbDriverBuilder;
    pub use rktk_drivers_common::usb::UsbOpts;
}

use embassy_nrf::{bind_interrupts, peripherals::USBD};

bind_interrupts!(pub struct Irqs {
    USBD => embassy_nrf::usb::InterruptHandler<USBD>;
    SPI2 => embassy_nrf::spim::InterruptHandler<SPI2>;
    TWISPI0 => embassy_nrf::twim::InterruptHandler<embassy_nrf::peripherals::TWISPI0>;
    UARTE0 => embassy_nrf::buffered_uarte::InterruptHandler<embassy_nrf::peripherals::UARTE0>;
});

static SOFTWARE_VBUS: OnceCell<SoftwareVbusDetect> = OnceCell::new();

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = embassy_nrf::config::Config::default();
    config.gpiote_interrupt_priority = Priority::P2;
    config.time_interrupt_priority = Priority::P2;
    let p = embassy_nrf::init(config);

    interrupt::USBD.set_priority(Priority::P2);
    interrupt::SPI2.set_priority(Priority::P2);
    interrupt::TWISPI0.set_priority(Priority::P2);
    interrupt::UARTE0.set_priority(Priority::P2);

    let display = Ssd1306DisplayBuilder::new(
        Twim::new(
            p.TWISPI0,
            Irqs,
            p.P0_17,
            p.P0_20,
            rktk_drivers_nrf::display::ssd1306::recommended_i2c_config(),
        ),
        ssd1306::size::DisplaySize128x32,
    );

    let Some(display) = panic_utils::display_message_if_panicked(display).await else {
        cortex_m::asm::udf()
    };

    let spi = Mutex::<NoopRawMutex, _>::new(Spim::new(
        p.SPI2,
        Irqs,
        p.P1_13,
        p.P1_11,
        p.P0_10,
        rktk_drivers_nrf::mouse::pmw3360::recommended_spi_config(),
    ));
    let ball_spi_device = SpiDevice::new(
        &spi,
        Output::new(
            p.P0_06,
            embassy_nrf::gpio::Level::High,
            embassy_nrf::gpio::OutputDrive::Standard,
        ),
    );
    let ball = Pmw3360Builder::new(ball_spi_device);

    let keyscan = DuplexMatrixScanner::<_, 5, 4, 7, 5>::new(
        [
            NrfFlexPin::new(p.P0_22), // ROW0
            NrfFlexPin::new(p.P0_24), // ROW1
            NrfFlexPin::new(p.P1_00), // ROW2
            NrfFlexPin::new(p.P0_11), // ROW3
            NrfFlexPin::new(p.P1_04), // ROW4
        ],
        [
            NrfFlexPin::new(p.P0_31), // COL0
            NrfFlexPin::new(p.P0_29), // COL1
            NrfFlexPin::new(p.P0_02), // COL2
            NrfFlexPin::new(p.P1_15), // COL3
        ],
        HandDetector::Constant(rktk::drivers::interface::keyscan::Hand::Left),
        None,
        translate_key_position,
    );

    let split = UartHalfDuplexSplitDriver::new(
        p.P0_08.degrade(),
        p.UARTE0,
        Irqs,
        p.TIMER1,
        p.PPI_CH0,
        p.PPI_CH1,
        p.PPI_GROUP0.degrade(),
    );

    let rgb = Ws2812Pwm::new(p.PWM0, p.P0_09);

    let sd = rktk_drivers_nrf::softdevice::init_softdevice("keyball61");

    #[cfg(feature = "ble")]
    let server = ble::init_ble_server(
        sd,
        rktk_drivers_nrf::softdevice::ble::DeviceInformation {
            manufacturer_name: Some("nazo6"),
            model_number: Some("100"),
            serial_number: Some("100"),
            ..Default::default()
        },
    );

    rktk_drivers_nrf::softdevice::start_softdevice(sd).await;

    embassy_time::Timer::after_millis(50).await;

    // let rand = rktk_drivers_nrf52::softdevice::rand::SdRand::new(sd);

    let (flash, cache) = get_flash(sd);
    let storage = rktk_drivers_nrf::softdevice::flash::create_storage_driver(flash, &cache);

    let ble_builder = {
        #[cfg(feature = "ble")]
        let ble = Some(ble::NrfBleDriverBuilder::new(
            sd,
            server,
            "keyball61",
            flash,
        ));

        #[cfg(not(feature = "ble"))]
        let ble = none_driver!(BleBuilder);

        ble
    };

    let drivers = Drivers {
        keyscan,
        system: NrfSystemDriver::new(None),
        mouse_builder: Some(ball),
        usb_builder: {
            #[cfg(feature = "usb")]
            let usb = {
                let vbus = SOFTWARE_VBUS.get_or_init(|| SoftwareVbusDetect::new(true, true));
                let driver = embassy_nrf::usb::Driver::new(p.USBD, Irqs, vbus);
                let opts = usb::UsbOpts {
                    config: USB_CONFIG,
                    mouse_poll_interval: 2,
                    kb_poll_interval: 5,
                    driver,
                };
                Some(usb::CommonUsbDriverBuilder::new(opts))
            };

            #[cfg(not(feature = "usb"))]
            let usb = none_driver!(UsbBuilder);

            usb
        },
        display_builder: Some(display),
        split: Some(split),
        rgb: Some(rgb),
        storage: Some(storage),
        ble_builder,
        debounce: Some(EagerDebounceDriver::new(
            embassy_time::Duration::from_millis(20),
        )),
        encoder: none_driver!(Encoder),
    };

    rktk::task::start(drivers, keymap::KEYMAP, create_empty_hooks()).await;
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    cortex_m::interrupt::disable();
    panic_utils::save_panic_info(info);
    cortex_m::peripheral::SCB::sys_reset()
}
