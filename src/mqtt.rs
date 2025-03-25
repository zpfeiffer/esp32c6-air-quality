use defmt::{debug, error, expect, info, warn};
use embassy_futures::select::{select, Either};
use embassy_net::{tcp::TcpSocket, Stack};
use embassy_time::{Duration, Timer, WithTimeout};
use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig},
    packet::v5::{publish_packet::QualityOfService, reason_codes::ReasonCode},
    utils::rng_generator::CountingRng,
};
use serde_json_core::ser::Error::BufferFull;
use smoltcp::wire::DnsQueryType;

use crate::bme680;
use crate::scd41;

#[embassy_executor::task]
pub async fn client(stack: Stack<'static>) {
    let mut scd41_receiver = expect!(
        scd41::WATCH.receiver(),
        "SCD41 Watch should have capacity for MQTT Receiver"
    );
    let mut bme680_receiver = expect!(
        bme680::WATCH.receiver(),
        "BME680 Watch should have capacity for MQTT Receiver"
    );

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
        config.add_max_subscribe_qos(QualityOfService::QoS1);
        config.add_username("air");
        config.add_password("123456");
        config.max_packet_size = 1024;
        let mut recv_buffer = [0; 2048];
        let mut write_buffer = [0; 2048];

        let mut client = MqttClient::<_, 10, _>::new(
            socket,
            &mut write_buffer,
            2048,
            &mut recv_buffer,
            2048,
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

        // TODO: this loop needs work
        while let Ok(val) = select(scd41_receiver.changed(), bme680_receiver.changed())
            .with_timeout(Duration::from_secs(5))
            .await
            .map_err(|_timeout_error| error!("MQTT: timed out waiting for sensor value"))
        {
            debug!("MQTT: receiver got val: {:?}", val);

            // Serialize the message to JSON
            let mut buf = [0u8; 512];
            let (topic, serialization_result) = match val {
                Either::First(scd41_measurement) => (
                    "air/scd41",
                    serde_json_core::to_slice(&scd41_measurement, &mut buf),
                ),
                Either::Second(bme680_measurement) => (
                    "air/bme680",
                    serde_json_core::to_slice(&bme680_measurement, &mut buf),
                ),
            };
            let message = match serialization_result
                .map(|size| &buf[..size])
                .map_err(|err| match err {
                    BufferFull => error!("MQTT: serialized value exceeded 512 bytes"),
                    error => error!("MQTT: serialization error: {:?}", error),
                }) {
                Ok(message) => message,
                Err(()) => continue, // Can't send this value, try again with the next
            };
            debug!("MQTT: created payload of size: {} bytes", message.len());

            // Send the message
            match client
                .send_message(topic, message, QualityOfService::QoS1, false)
                .await
            {
                Ok(()) => info!("MQTT: message sent successfully!"),
                Err(ReasonCode::NoMatchingSubscribers) => {
                    error!("MQTT: no matching subscribers");
                    continue; // Not our fault, so we'll try again with the next value
                }
                Err(err) => {
                    error!("MQTT: error sending message: {:?}", err);
                    break; // Re-connect to broker
                }
            }
        }

        // The inner loop runs until an error is encountered sending the message
        warn!("MQTT: re-connecting to broker due to error");
    }
}
