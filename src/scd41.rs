use defmt::{debug, error, info, trace, Format};
use embassy_time::{Delay, Duration, Timer};
use esp_hal::{
    gpio::AnyPin,
    i2c::master::{AnyI2c, I2c},
};
use scd4x::Scd4xAsync;

#[derive(Debug, Format)]
pub struct AirQuality {
    // TODO
    timestamp: Option<()>,
    co2: u16,
    temperature: f32,
    humidity: f32,
}

#[embassy_executor::task]
pub async fn scd41_sensor_task(i2c_peripheral: AnyI2c, sda: AnyPin, sdc: AnyPin) {
    let i2c = I2c::new(i2c_peripheral, Default::default())
        .expect("i2c config should be valid")
        .with_sda(sda)
        .with_scl(sdc)
        .into_async();

    let mut sensor = Scd4xAsync::new(i2c, Delay);

    // Sensor does not acknowledge wake-up
    trace!("sending wake-up to SCD41...");
    sensor.wake_up().await;

    // Return to known state
    match sensor.stop_periodic_measurement().await {
        Ok(()) => debug!("SCD41: stopped periodic measurement"),
        Err(_) => error!("SCD41: failed to stop periodic measurement"),
    }

    let serial_number = sensor.serial_number().await;
    match serial_number {
        Ok(num) => info!("SCD41: serial number: {:04x}", num),
        Err(_) => error!("SCD41: failed to get SCD41 serial number"),
    };

    let temp_offset = sensor.temperature_offset().await;
    match temp_offset {
        Ok(offset) => info!("SCD41: temperature offset: {}", offset),
        Err(_) => error!("SCD41: failed to get temperature offset"),
    };

    // TODO: set altitude

    // TODO: persist settings?

    match sensor.start_periodic_measurement().await {
        Ok(()) => info!("SCD41: started periodic measurement"),
        Err(_) => error!("SCD41: failed to start periodic measurement"),
    };

    loop {
        Timer::after(Duration::from_secs(5)).await;

        let measurement = sensor.measurement().await.map(|data| AirQuality {
            timestamp: None,
            co2: data.co2,
            temperature: data.temperature,
            humidity: data.humidity,
        });

        match measurement {
            Ok(data) => info!("SCD41: got measurement: {:?}", data),
            Err(_) => error!("SCD41: failed to read measurement"),
        };
    }
}
