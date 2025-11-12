use core::fmt::Debug;

use bosch_bme680::{AsyncBme680, Configuration, MeasurmentData};
use defmt::{debug, error, expect, info, warn, Format};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    watch::Watch,
};
use embassy_time::{Delay, Timer};
use esp_hal::{i2c::master::I2c, Async};
use serde::Serialize;

pub static WATCH: Watch<CriticalSectionRawMutex, Bme680Measurement, 2> = Watch::new();

#[derive(Debug, Format, Clone, Serialize)]
pub struct Bme680Measurement {
    /// Temperature in °C
    pub temperature: f32,
    /// Relative humidity in %
    pub humidity: f32,
    /// Pressure in hPa
    pub pressure: f32,
    /// Gas resistance in Ohms
    /// None if gas measurment is disabled or gas measurment hasn't finished in time according to the gas_measuring bit.
    pub gas_resistance: Option<f32>,
}

#[embassy_executor::task]
pub async fn bme680_sensor_task(
    i2c_device: I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>,
) -> ! {
    let device_address = bosch_bme680::DeviceAddress::Secondary;
    let mut sensor = AsyncBme680::new(i2c_device, device_address, Delay, 23);
    let sensor_config = Configuration::builder().build();

    debug!("BME680: initializing sensor...");
    expect!(
        sensor.initialize(&sensor_config).await,
        "BME680: failed to initialize sensor"
    );
    info!("BME680: initialized successfully");

    let sender = WATCH.sender();
    debug!("BME680: obtained Sender for Watch");

    loop {
        Timer::after_secs(2).await;

        debug!("BME680: triggering measurement...");
        let measurement = match sensor.measure().await {
            Ok(MeasurmentData {
                temperature,
                humidity,
                pressure,
                gas_resistance,
            }) => Bme680Measurement {
                temperature,
                humidity,
                // bosch-bme680 crate docs are wrong, pressure is returned
                // in Pa, not hPa. TODO: contribute upstream
                pressure: pressure / 100.0, // Convert Pa to hPa
                gas_resistance,
            },
            Err(_error) => {
                // TODO: print error
                error!("BME680: failed to get measurement");
                continue;
            }
        };

        info!("BME680: got measurement: {:?}", measurement);

        if measurement.pressure <= 300.0 || measurement.pressure >= 1100.0 {
            warn!(
                "BME680: pressue measurement outside of accurate range: {} hPa",
                measurement.pressure
            );
        }

        // Update consumers
        sender.send(measurement);
        debug!("BME680: sent measurement to Watch")
    }
}
