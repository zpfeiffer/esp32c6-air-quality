use defmt::{debug, error, expect, info, warn, Format};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    watch::Watch,
};
use embassy_time::{Delay, Duration, Timer};
use esp_hal::{i2c::master::I2c, Async};
use scd4x::Scd4xAsync;
use serde::Serialize;

use crate::bme680::{self, Bme680Measurement};

pub static WATCH: Watch<CriticalSectionRawMutex, Scd41Measurement, 2> = Watch::new();

#[derive(Debug, Format, Clone, Serialize)]
pub struct Scd41Measurement {
    // TODO
    timestamp: Option<()>,
    pub co2: u16,
    temperature: f32,
    humidity: f32,
}

/// Supervisor task that inititalizes the SCD41 sensor task and restarts
/// it if it fails.
#[embassy_executor::task]
pub async fn supervisor(i2c_device: I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>) -> ! {
    let mut sensor = Scd4xAsync::new(i2c_device, Delay);

    loop {
        info!("SCD41: starting sensor task...");
        let _ = scd41_sensor_task(&mut sensor).await;
        error!("SCD41: sensor failed. restarting...");
        Timer::after_secs(1).await;
    }
}

async fn scd41_sensor_task(
    sensor: &mut Scd4xAsync<I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>, Delay>,
) -> Result<!, ()> {
    debug!("SCD41: sending wake-up...");
    sensor.wake_up().await; // Sensor does not acknowledge wake-up

    // Return to known state
    match sensor.stop_periodic_measurement().await {
        Ok(()) => debug!("SCD41: stopped periodic measurement"),
        Err(_) => Err(error!("SCD41: failed to stop periodic measurement"))?,
    }

    match sensor.serial_number().await {
        Ok(serial_number) => info!("SCD41: serial number: {:04x}", serial_number),
        Err(_) => Err(error!("SCD41: failed to get SCD41 serial number"))?,
    };

    match sensor.temperature_offset().await {
        Ok(temp_offset) => info!("SCD41: temperature offset: {}", temp_offset),
        Err(_) => Err(error!("SCD41: failed to get temperature offset"))?,
    };

    let mut bme680_receiver = expect!(
        bme680::WATCH.receiver(),
        "BME680 Watch should have capacity for SCD41 receiver"
    );

    // TODO: automatic self calibration?

    // TODO: persist settings? or re-init settings on start?

    match sensor.start_periodic_measurement().await {
        Ok(()) => info!("SCD41: started periodic measurement"),
        Err(_) => Err(error!("SCD41: failed to start periodic measurement"))?,
    };

    let sender = WATCH.sender();
    debug!("SCD41: obtained Sender for Watch");

    loop {
        Timer::after(Duration::from_secs(5)).await;

        // Get latest pressure from BME680 and validate
        let pressure_hpa = match bme680_receiver
            .try_get()
            .map(|measurement: Bme680Measurement| measurement.pressure)
        {
            Some(pressure) if pressure >= 700.0 && pressure <= 1200.0 => {
                debug!("SCD41: got pressure measurement: {} hPa", pressure);
                pressure as u16
            }
            Some(pressure) => {
                warn!("SCD41: got invalid pressure measurement: {} hPa", pressure);
                if pressure.is_finite() {
                    let clamped = pressure.clamp(700.0, 1200.0) as u16;
                    warn!("SCD41: using clamped measurement: {} hPa", clamped);
                    clamped
                } else {
                    warn!("SCD41: using fallback measurement: 1015 hPa");
                    1015
                }
            }
            None => {
                error!("SCD41: no BME680 pressure data available, using fallback: 1015 hPa");
                1015
            }
        };

        match sensor.set_ambient_pressure(pressure_hpa).await {
            Ok(()) => debug!("SCD41: set ambient pressure to {} hPa", pressure_hpa),
            Err(_) => Err(error!("SCD41: failed to set ambient pressure"))?,
        }

        let measurement = sensor
            .measurement()
            .await
            .map(|data| Scd41Measurement {
                timestamp: None,
                co2: data.co2,
                temperature: data.temperature,
                humidity: data.humidity,
            })
            .map_err(|_| error!("SCD41: failed to read measurement"))?;

        info!("SCD41: got measurement: {:?}", measurement);

        // Update consumers
        sender.send(measurement);
        debug!("SCD41: sent measurement to Watch")
    }
}
