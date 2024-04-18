use embassy_futures::{
    join::join,
    select::{select, Either},
};
use embassy_sync::channel::Channel;
use embassy_time::Timer;

use crate::{
    constant::SPLIT_USB_TIMEOUT,
    display::DISPLAY,
    driver::{ball, keyboard},
    usb::Hid,
};

use super::{led_task::LedCtrlTx, BallPeripherals, KeyboardPeripherals, SplitPeripherals};

mod master;
mod slave;

mod split;

pub async fn start(
    ball_peripherals: BallPeripherals,
    keyboard_peripherals: KeyboardPeripherals,
    split_peripherals: SplitPeripherals,
    led_controller: LedCtrlTx<'_>,
    mut hid: Hid<'_>,
) {
    // VBUS detection is not available for ProMicro RP2040, so USB communication is used to determine master/slave.
    // This is same as SPLIT_USB_DETECT in QMK.
    let is_master = match select(hid.keyboard.ready(), Timer::after_millis(SPLIT_USB_TIMEOUT)).await
    {
        Either::First(_) => true,
        Either::Second(_) => false,
    };

    let s2m_chan: split::S2mChannel = Channel::new();
    let s2m_tx = s2m_chan.sender();
    let s2m_rx = s2m_chan.receiver();

    let m2s_chan: split::M2sChannel = Channel::new();
    let m2s_tx = m2s_chan.sender();
    let m2s_rx = m2s_chan.receiver();

    let ball = ball::Ball::init(ball_peripherals).await.ok();
    let keyboard = keyboard::Keyboard::new(keyboard_peripherals);

    DISPLAY.clear().await;
    DISPLAY.set_mouse(ball.is_some()).await;

    #[cfg(feature = "force-master")]
    {
        join(
            master::start(hid, ball, keyboard, s2m_rx, m2s_tx),
            split::master_split_handle(split_peripherals, m2s_rx, s2m_tx),
        )
        .await;
        return;
    }

    #[cfg(feature = "force-slave")]
    {
        join(
            slave::start(ball, keyboard, m2s_rx, s2m_tx),
            split::slave_split_handle(split_peripherals, m2s_tx, s2m_rx),
        )
        .await;
        return;
    }

    if is_master {
        join(
            master::start(hid, ball, keyboard, s2m_rx, m2s_tx),
            split::master_split_handle(split_peripherals, m2s_rx, s2m_tx),
        )
        .await;
    } else {
        join(
            slave::start(ball, keyboard, m2s_rx, s2m_tx),
            split::slave_split_handle(split_peripherals, m2s_tx, s2m_rx),
        )
        .await;
    }
}
