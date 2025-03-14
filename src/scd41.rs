use defmt::{debug, error, info, Format};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Watch};
use embassy_time::{Delay, Duration, Timer};
use esp_hal::{
    gpio::AnyPin,
    i2c::master::{AnyI2c, I2c},
    Async,
};
use scd4x::Scd4xAsync;

pub static WATCH: Watch<CriticalSectionRawMutex, AirQuality, 2> = Watch::new();

#[derive(Debug, Format, Clone)]
pub struct AirQuality {
    // TODO
    timestamp: Option<()>,
    pub co2: u16,
    temperature: f32,
    humidity: f32,
}

/// Supervisor task that inititalizes the SCD41 sensor task and restarts
/// it if it fails.
#[embassy_executor::task]
pub async fn supervisor(i2c_peripheral: AnyI2c, sda: AnyPin, sdc: AnyPin) -> ! {
    let i2c = I2c::new(i2c_peripheral, Default::default())
        .expect("i2c config should be valid")
        .with_sda(sda)
        .with_scl(sdc)
        .into_async();

    let mut sensor = Scd4xAsync::new(i2c, Delay);

    loop {
        info!("SCD41: starting sensor task...");
        let _ = scd41_sensor_task(&mut sensor).await;
        error!("SCD41: sensor failed. restarting...");
        Timer::after_secs(1).await;
    }
}

async fn scd41_sensor_task(sensor: &mut Scd4xAsync<I2c<'_, Async>, Delay>) -> Result<!, ()> {
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

    // TODO: set altitude

    // TODO: persist settings? or re-init settings on start?

    match sensor.start_periodic_measurement().await {
        Ok(()) => info!("SCD41: started periodic measurement"),
        Err(_) => Err(error!("SCD41: failed to start periodic measurement"))?,
    };

    let sender = WATCH.sender();
    debug!("SCD41: obtained Sender for Watch");

    loop {
        Timer::after(Duration::from_secs(5)).await;

        let measurement = sensor
            .measurement()
            .await
            .map(|data| AirQuality {
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
