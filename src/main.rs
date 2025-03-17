#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![feature(never_type)]

#[deny(clippy::mem_forget)]
use defmt::info;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::AnyPin;
use esp_hal::i2c::master::{AnyI2c, I2c};
use esp_hal::rmt::Rmt;
use esp_hal::rng::Rng;
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::Async;
use esp_wifi::EspWifiController;
use led::SmartLedsAdapter;
use panic_rtt_target as _;
use smart_leds::hsv::{hsv2rgb, Hsv};
use smart_leds::{brightness, gamma, SmartLedsWrite};
use static_cell::StaticCell;
use wifi::wifi_init;

extern crate alloc;

mod led;
mod mqtt;
mod scd41;
mod wifi;

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

    static RNG: StaticCell<Rng> = StaticCell::new();
    let rng = RNG.init_with(|| esp_hal::rng::Rng::new(peripherals.RNG));

    // Random seed for embassy_net stack
    let network_seed = (rng.random() as u64) << 32 | rng.random() as u64;

    static ESP_WIFI_CONTROLLER: StaticCell<EspWifiController<'static>> = StaticCell::new();
    let esp_wifi_controller = ESP_WIFI_CONTROLLER
        .init_with(|| esp_wifi::init(timer1.timer0, rng.clone(), peripherals.RADIO_CLK).unwrap());

    let stack = wifi_init(esp_wifi_controller, peripherals.WIFI, spawner, network_seed).await;

    spawner.must_spawn(mqtt::client(stack));

    static I2C_BUS: StaticCell<Mutex<NoopRawMutex, I2c<'static, Async>>> = StaticCell::new();

    let i2c_peripheral: AnyI2c = peripherals.I2C0.into();
    let sda: AnyPin = peripherals.GPIO3.into();
    let sdc: AnyPin = peripherals.GPIO23.into();
    let i2c = I2c::new(i2c_peripheral, Default::default())
        .expect("i2c config should be valid")
        .with_sda(sda)
        .with_scl(sdc)
        .into_async();
    let i2c_bus = Mutex::new(i2c);
    let i2c_bus = I2C_BUS.init(i2c_bus);
    let i2c_dev1 = I2cDevice::new(i2c_bus);

    spawner.must_spawn(scd41::supervisor(i2c_dev1));

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

            Timer::after_millis(20).await;
        }
    }
}
