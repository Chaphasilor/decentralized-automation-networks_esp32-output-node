use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
use esp_idf_hal::{delay::FreeRtos, gpio::PinDriver, peripherals::Peripherals};
use log::*;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
};
use anyhow::{Result};
use std::{net::{UdpSocket, SocketAddr}, time::{SystemTime, UNIX_EPOCH}};

pub mod wifi;

use crate::wifi::wifi;

#[toml_cfg::toml_config]
pub struct Config {
    #[default("")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_psk: &'static str,
    #[default(21001)]
    port: u16,
}

fn main() -> Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_sys::link_patches();
    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    // println!("Hello, world!");

    // // Get all the peripherals
    // let peripherals = Peripherals::take().unwrap();
    // // Initialize Pin 8 as an output to drive the LED
    // let mut led = peripherals.pins.gpio4.into_output().unwrap();

    // !!! pins that should not be used on ESP32: 6-11, 16-17
    // those are reserved for SPI and trying to use them will result in a watchdog timer reset
    let peripherals = Peripherals::take().unwrap();

    let sysloop = EspSystemEventLoop::take()?;

    let mut led_pin = PinDriver::output(peripherals.pins.gpio12).unwrap();

    // The constant `CONFIG` is auto-generated by `toml_config`.
    let app_config = CONFIG;

    info!("SSID: {}", app_config.wifi_ssid);
    info!("PSK: {}", app_config.wifi_psk);
    
    // Connect to the Wi-Fi network
    let wifi_interface = wifi(
        app_config.wifi_ssid,
        app_config.wifi_psk,
        peripherals.modem,
        sysloop,
    ).expect("Couldn't connect to WiFi!");

    info!("IP (sta): {}", wifi_interface.sta_netif().get_ip_info()?.ip);
    info!("IP (ap): {}", wifi_interface.ap_netif().get_ip_info()?.ip);

    let socket_address = SocketAddr::from(([0, 0, 0, 0], app_config.port));
    info!("socket address created");
    let socket = UdpSocket::bind(socket_address)?;
    info!("socket bound");

    let mut buffer = [0; 2048];
    info!("buffer allocated");

    loop {

        let (n, addr) = socket.recv_from(&mut buffer)?;
        // let message = String::from_utf8(buffer.into())?;
        let message = String::from_utf8_lossy(&buffer[..n]).into_owned(); // !!! from_utf8() triggers a reset
        let received = SystemTime::now();
        info!("received message from '{source}' at {received}: {message}", source=addr,  received=received.duration_since(UNIX_EPOCH).unwrap().as_millis(), message=message);

        let json: serde_json::Value = serde_json::from_str(&message).expect("Couldn't parse JSON");
        if let Some(message_type) = json["type"].as_str() {
            match message_type {
                "udpPing" => {
                    let start = SystemTime::now();
                    let time = start.duration_since(std::time::UNIX_EPOCH).expect("Couldn't get system time");
                    let return_buf = (time.as_micros() as u64).to_be_bytes();
                    let return_address = json["replyTo"].as_str().unwrap().parse::<SocketAddr>().expect("No return address given");

                    // send current system time back to sender
                    socket.send_to(&return_buf, return_address).unwrap();
                    warn!("Sent UDP ping response to {}", return_address);
                },
                _ => {
                    info!("Received message: {}", json);
                    // Inverse logic to turn LED on
                    led_pin.set_low().unwrap();
                    info!("LED ON");
                    FreeRtos::delay_ms(100);

                    led_pin.set_high().unwrap();
                    info!("LED OFF");
                    FreeRtos::delay_ms(100);
                }
            }
        }

    }

}
