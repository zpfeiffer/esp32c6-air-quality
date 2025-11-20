fn main() {
    linker_be_nice();
    load_config();
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

fn load_config() {
    use config::{Config, File};
    use std::path::Path;

    let config_path = Path::new("config.toml");

    if !config_path.exists() {
        eprintln!("config.toml not found");
        return;
    }

    let settings = Config::builder()
        .add_source(File::with_name("config"))
        .build()
        .expect("Failed to load config.toml");

    // WiFi settings
    if let Ok(ssid) = settings.get_string("wifi.ssid") {
        println!("cargo:rustc-env=SSID={}", ssid);
    }
    if let Ok(psk) = settings.get_string("wifi.psk") {
        println!("cargo:rustc-env=PSK={}", psk);
    }

    // MQTT settings
    if let Ok(host) = settings.get_string("mqtt.host") {
        println!("cargo:rustc-env=MQTT_HOST={}", host);
    }
    if let Ok(port) = settings.get_int("mqtt.port") {
        println!("cargo:rustc-env=MQTT_PORT={}", port);
    }
    if let Ok(username) = settings.get_string("mqtt.username") {
        println!("cargo:rustc-env=MQTT_USERNAME={}", username);
    }
    if let Ok(password) = settings.get_string("mqtt.password") {
        println!("cargo:rustc-env=MQTT_PASSWORD={}", password);
    }
    if let Ok(topic_scd41) = settings.get_string("mqtt.topic_scd41") {
        println!("cargo:rustc-env=MQTT_TOPIC_SCD41={}", topic_scd41);
    }
    if let Ok(topic_bme680) = settings.get_string("mqtt.topic_bme680") {
        println!("cargo:rustc-env=MQTT_TOPIC_BME680={}", topic_bme680);
    }

    println!("cargo:rerun-if-changed=config.toml");
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                "_defmt_timestamp" => {
                    eprintln!();
                    eprintln!("💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`");
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                _ => (),
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
