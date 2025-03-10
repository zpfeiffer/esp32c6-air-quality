#![no_std]
#![no_main]

use air::led::SmartLedsAdapter;
use air::scd41::Scd41;
use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::i2c::master::{Config, I2c};
use esp_hal::peripheral::Peripheral;
use esp_hal::rmt::Rmt;
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;
use panic_rtt_target as _;
use smart_leds::hsv::{hsv2rgb, Hsv};
use smart_leds::{brightness, gamma, SmartLedsWrite};

extern crate alloc;

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.3.0

    rtt_target::rtt_init_defmt!();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 72 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    let timer1 = TimerGroup::new(peripherals.TIMG0);
    let _init = esp_wifi::init(
        timer1.timer0,
        esp_hal::rng::Rng::new(peripherals.RNG),
        peripherals.RADIO_CLK,
    )
    .unwrap();

    // TODO: Spawn some tasks
    let _ = spawner;

    let mut scd41 = Scd41::init(peripherals.I2C0, peripherals.GPIO3, peripherals.GPIO2);
    let serial_number = scd41.sensor.serial_number().await;
    match serial_number {
        Ok(num) => info!("got serial number: {}", num),
        Err(_) => error!("failed to get serial number!"),
    };

    let led_pin = peripherals.GPIO8;
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80)).unwrap();

    // Num LEDs (1) * num channels (r,g,b -> 3) * pulses per channel (8) = 24
    // + 1 additional pulse for end delimiter = 25
    let rmt_buffer = [0u32; 25];

    let mut led = SmartLedsAdapter::new(rmt.channel0, led_pin, rmt_buffer);

    let mut color = Hsv {
        hue: 0,
        sat: 255,
        val: 255,
    };

    loop {
        info!("Hello world!");
        for hue in 0..=255 {
            color.hue = hue;

            // Convert from HSV to RGB color space
            let rgb_data = [hsv2rgb(color)];

            // Apply gamma correction
            let gamma_corrected = gamma(rgb_data.into_iter());

            // Limit brightness to 10/255
            let brightness_limited = brightness(gamma_corrected, 10);

            // Start RMT operation
            led.write(brightness_limited).unwrap();

            Timer::after(Duration::from_millis(20)).await;
        }
    }
}
