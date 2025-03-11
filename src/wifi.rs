use defmt::{debug, error, info};
use embassy_executor::Spawner;
use embassy_net::{Runner, StackResources};
use embassy_time::{Duration, Timer};
use esp_hal::{peripherals::WIFI, rng::Rng};
use esp_wifi::{
    wifi::{ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiState},
    EspWifiController,
};

use static_cell::StaticCell;

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PSK");

pub async fn wifi_init(
    esp_wifi_controller: &'static mut EspWifiController<'static>,
    wifi_peripheral: WIFI,
    spawner: Spawner,
    rng: &'static mut Rng,
) {
    // TODO: move to main task
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let (controller, interfaces) =
        esp_wifi::wifi::new(esp_wifi_controller, wifi_peripheral).unwrap();

    let wifi_interface = interfaces.sta;

    let config = embassy_net::Config::dhcpv4(Default::default());

    // Init network stack
    static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    let resources = RESOURCES.init_with(|| StackResources::<3>::new());
    let (stack, runner) = embassy_net::new(wifi_interface, config, resources, seed);

    spawner.must_spawn(connection(controller));
    spawner.must_spawn(net_task(runner));

    while !stack.is_link_up() {
        Timer::after(Duration::from_millis(500)).await;
    }

    debug!("waiting to get IP address...");
    loop {
        if let Some(config) = stack.config_v4() {
            info!("got IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    debug!("start connection task");
    for capability in controller.capabilities().unwrap() {
        info!("WiFi controller reports capability: {:?}", capability);
    }
    loop {
        match esp_wifi::wifi::wifi_state() {
            WifiState::StaConnected => {
                // wait until we're no longer connected
                controller.wait_for_event(WifiEvent::StaDisconnected).await;
                Timer::after(Duration::from_millis(5000)).await
            }
            _ => {}
        }

        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = Configuration::Client(ClientConfiguration {
                ssid: SSID.try_into().unwrap(),
                password: PASSWORD.try_into().unwrap(),
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            info!("starting WiFi...");
            controller.start_async().await.unwrap();
            info!("WiFi started!");
        }

        debug!("WiFi: about to connect...");

        match controller.connect_async().await {
            Ok(_) => info!("Wifi connected!"),
            Err(e) => {
                error!("failed to connect to WiFi: {:?}", e);
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}
