use core::fmt::Write;

use embassy_rp::peripherals::{PIN_1, PIO0};
use embassy_rp::pio::{Common, Config, Pin, Pio, ShiftDirection, StateMachine};
use embassy_time::Timer;
use fixed::traits::ToFixed;

use crate::DISPLAY;

// const BITRATE: f64 = 115200.0;
//                                               ↓ 一つのPIOループで138クロック
//                                                       ↓ 1つのPIO命令で32ビット送信
// const DIVIDER: f64 = 125_000_000.0 / (BITRATE * (138.0 / 32.0));

const DIVIDER: f64 = 128.0;

fn rx_init<'a>(
    common: &mut Common<'a, PIO0>,
    sm: &mut StateMachine<'a, PIO0, 0>,
    data_pin: &Pin<'a, PIO0>,
) {
    let prg = pio_proc::pio_asm!(
        "set pindirs 0",
        ".wrap_target",
        "wait 0 pin 0",
        "set x 7 [8]",
        "bitloop:",
        "in pins 1 [6]",
        "jmp x-- bitloop",
        "push",
        ".wrap"
    );
    let mut cfg = Config::default();
    cfg.use_program(&common.load_program(&prg.program), &[]);

    cfg.set_in_pins(&[data_pin]);

    cfg.shift_in.direction = ShiftDirection::Left;

    cfg.fifo_join = embassy_rp::pio::FifoJoin::RxOnly;

    cfg.clock_divider = DIVIDER.to_fixed();
    sm.set_config(&cfg);
    sm.set_enable(true);
}

fn tx_init<'a>(
    common: &mut Common<'a, PIO0>,
    sm: &mut StateMachine<'a, PIO0, 1>,
    data_pin: &Pin<'a, PIO0>,
) {
    let prg = pio_proc::pio_asm!(
        "set pindirs 0",
        ".wrap_target",
        "pull",
        "set x 7 [2]",
        "set pins 0",
        "set pindirs 1 [7]",
        "bitloop:",
        "out pins 1 [6]",
        "jmp x-- bitloop",
        "set pins 1",
        "set pindirs 0 [2]",
        ".wrap"
    );
    let mut cfg = Config::default();
    cfg.use_program(&common.load_program(&prg.program), &[]);

    cfg.set_out_pins(&[data_pin]);
    cfg.set_set_pins(&[data_pin]);

    cfg.shift_out.direction = ShiftDirection::Left;

    cfg.fifo_join = embassy_rp::pio::FifoJoin::TxOnly;

    cfg.clock_divider = DIVIDER.to_fixed();
    sm.set_config(&cfg);
    sm.set_enable(false);
}

pub struct Communicate<'a> {
    rx_sm: StateMachine<'a, PIO0, 0>,
    tx_sm: StateMachine<'a, PIO0, 1>,
}

impl<'a> Communicate<'a> {
    pub fn new<'b: 'a>(pio: Pio<'b, PIO0>, data_pin: PIN_1) -> Communicate<'a> {
        let mut common = pio.common;
        let mut sm0 = pio.sm0;
        let mut sm1 = pio.sm1;

        let mut out_pin = common.make_pio_pin(data_pin);
        out_pin.set_pull(embassy_rp::gpio::Pull::Up);

        rx_init(&mut common, &mut sm0, &out_pin);
        tx_init(&mut common, &mut sm1, &out_pin);

        Self {
            rx_sm: sm0,
            tx_sm: sm1,
        }
    }
    pub async fn recv_data<const N: usize>(&mut self, buf: &mut [u8; N]) {
        let mut i = 0;
        while i < buf.len() {
            let data = self.rx_sm.rx().wait_pull().await;

            let mut str = heapless::String::<512>::new();
            write!(str, "recv: {:X}", data).unwrap();
            DISPLAY.lock().await.as_mut().unwrap().draw_text(&str);

            buf[i] = data as u8;
            i += 1;
        }
    }

    pub async fn send_data<const N: usize>(&mut self, buf: &[u8]) {
        let mut i = 0;
        self.rx_sm.set_enable(false);
        Timer::after_millis(10).await;
        self.tx_sm.restart();
        self.tx_sm.set_enable(true);
        while i < buf.len() {
            let data = buf[i] as u32;
            let data = data << 24;
            self.tx_sm.tx().wait_push(data).await;

            let mut str = heapless::String::<256>::new();
            write!(str, "sent: {:X}", data).unwrap();
            DISPLAY.lock().await.as_mut().unwrap().draw_text(&str);

            i += 1;
        }

        Timer::after_millis(5).await;
        self.tx_sm.set_enable(false);
        self.rx_sm.restart();
        self.rx_sm.set_enable(true);
    }
}