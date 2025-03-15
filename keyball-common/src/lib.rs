#![no_std]

pub mod keymap;

pub use keymap::KEYMAP;

use rktk::drivers::interface::keyscan::Hand;
use rktk_drivers_common::{keyscan::duplex_matrix::ScanDir, mouse::paw3395, usb::UsbDriverConfig};

pub const PAW3395_CONFIG: paw3395::config::Config = paw3395::config::Config {
    mode: paw3395::config::HP_MODE,
    lift_cutoff: paw3395::config::LiftCutoff::_2mm,
};

pub const USB_CONFIG: UsbDriverConfig = {
    let mut config = UsbDriverConfig::new(0xc0de, 0xcafe);

    config.manufacturer = Some("Yowkees/nazo6");
    config.product = Some("keyball");
    config.serial_number = Some("12345678");
    config.max_power = 100;
    config.max_packet_size_0 = 64;
    config.supports_remote_wakeup = true;

    config
};

// メモ: Keyballのキースキャン配線
//
// 左                   右
//
// 論理 0 1 2 3 4 5 6        0 1 2 3 4 5 6
// 物理
//      [C2R] [ R2C ]        [ R2C ] [C2R]
// COL→ 0 1 2 0 1 2 3   COL→ 3 2 1 0 2 1 0
// ROW↓                 ROW↓
// 0                    0
// 1                    1
// 2                    2
// 3                    3
// 4                    4
pub fn translate_key_position(
    hand: Hand,
) -> impl Fn(ScanDir, usize, usize) -> Option<(usize, usize)> {
    move |dir: ScanDir, row: usize, col: usize| match (hand, dir) {
        (Hand::Left, ScanDir::Col2Row) => {
            if col > 2 {
                None
            } else {
                Some((row, col))
            }
        }
        (Hand::Left, ScanDir::Row2Col) => Some((row, col + 3)),
        (Hand::Right, ScanDir::Row2Col) => Some((row, 3 - col)),
        (Hand::Right, ScanDir::Col2Row) => {
            if col > 2 {
                None
            } else {
                Some((row, 6 - col))
            }
        }
    }
}
