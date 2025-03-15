use defmt::{debug, error, info};
use embassy_net::{tcp::TcpSocket, Stack};
use embassy_time::{Duration, Timer, WithTimeout};
use heapless::String;
use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig},
    packet::v5::{publish_packet::QualityOfService, reason_codes::ReasonCode},
    utils::rng_generator::CountingRng,
};
use smoltcp::wire::DnsQueryType;

use core::fmt::Write;

use crate::scd41;

#[embassy_executor::task]
pub async fn client(stack: Stack<'static>) {
    let mut receiver = scd41::WATCH
        .receiver()
        .expect("SCD41 Watch should have capacity for MQTT Receiver");

    loop {
        let mut rx_buffer = [0; 4096];
        let mut tx_buffer = [0; 4096];

        Timer::after_secs(1).await;
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

        socket.set_timeout(Some(embassy_time::Duration::from_secs(10)));

        let address = match stack
            .dns_query("nas.local", DnsQueryType::A)
            .await
            .map(|a| a[0])
        {
            Ok(address) => address,
            Err(e) => {
                error!("DNS lookup error: {:?}", e);
                continue;
            }
        };
        info!("resolved nas.local to: {}", address);

        let remote_endpoint = (address, 1883);
        info!("connecting...");
        let connection = socket.connect(remote_endpoint).await;
        if let Err(e) = connection {
            error!("connect error: {:?}", e);
            continue;
        }
        info!("connected!");

        let mut config = ClientConfig::new(
            rust_mqtt::client::client_config::MqttVersion::MQTTv5,
            CountingRng(20000),
        );
        config.add_max_subscribe_qos(rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS1);
        config.add_username("air");
        config.add_password("123456");
        config.max_packet_size = 512;
        let mut recv_buffer = [0; 1024];
        let mut write_buffer = [0; 1024];

        let mut client = MqttClient::<_, 10, _>::new(
            socket,
            &mut write_buffer,
            1024,
            &mut recv_buffer,
            1024,
            config,
        );

        match client.connect_to_broker().await {
            Ok(()) => {
                info!("Connected to broker!")
            }
            Err(mqtt_error) => match mqtt_error {
                ReasonCode::NetworkError => {
                    error!("MQTT Network Error");
                    continue;
                }
                _ => {
                    error!("Other MQTT Error: {:?}", mqtt_error);
                    continue;
                }
            },
        }

        loop {
            if let Ok(val) = receiver
                .changed()
                .with_timeout(Duration::from_secs(5))
                .await
            {
                debug!("MQTT: receiver got val: {:?}", val);
                let mut message_string: String<5> = String::new();
                write!(message_string, "{}", val.co2).unwrap();
                debug!("MQTT: formatted message: {:?}", message_string);
                let message = message_string.as_bytes();

                match client
                    .send_message("air", message, QualityOfService::QoS1, false)
                    .await
                {
                    Ok(()) => info!("MQTT: message sent successfully!"),
                    Err(ReasonCode::NoMatchingSubscribers) => {
                        error!("MQTT: no matching subscribers")
                        // Not our fault, so keep trying
                    }
                    Err(err) => {
                        error!("Other MQTT Error: {:?}", err);
                        break;
                    }
                }
            } else {
                error!("MQTT: timed out waiting for SCD41 value");
            };
        }
    }
}
