use embassy_time::Delay;
use esp_hal::{
    gpio::interconnect::PeripheralOutput,
    i2c::master::{AnyI2c, I2c},
    peripheral::Peripheral,
    Async,
};
use scd4x::Scd4xAsync;

pub struct Scd41 {
    pub sensor: Scd4xAsync<I2c<'static, Async>, Delay>,
}

impl Scd41 {
    pub fn init(
        i2c_peripheral: impl Peripheral<P = impl Into<AnyI2c>> + 'static,
        sda: impl Peripheral<P = impl PeripheralOutput> + 'static,
        sdc: impl Peripheral<P = impl PeripheralOutput> + 'static,
    ) -> Self {
        let i2c = I2c::new(i2c_peripheral.map_into(), Default::default())
            .expect("i2c config should be valid")
            .with_sda(sda)
            .with_scl(sdc)
            .into_async();
        Self {
            sensor: Scd4xAsync::new(i2c, Delay),
        }
    }
}
