use chrono::prelude::*;
use ringbuffer::RingBuffer;
use rocket::response::content::RawJson;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    io::{BufRead, BufReader},
    sync::RwLock,
    time::Duration,
};

mod cors;

#[derive(Serialize, Deserialize, Clone)]
struct Temperature {
    temperature: f32,
    humidity: f32,
    #[serde(default = "Utc::now")]
    timestamp: DateTime<Utc>,
}

const BUFFER_SIZE: usize = 24 * 60; // one day of values, one per minute
type Buffer = ringbuffer::ConstGenericRingBuffer<Temperature, BUFFER_SIZE>;
static DATA_BUFFER: RwLock<Buffer> = RwLock::new(Buffer::new());
const ESP_BAUD_RATE: u32 = 115200;

#[rocket::get("/temperature")]
fn temperature() -> Option<RawJson<String>> {
    let buffer = DATA_BUFFER.read().unwrap();
    if let Some(value) = buffer.back() {
        let json_string = serde_json::to_string(value).expect("cannot serialize Temperature");
        Some(RawJson(json_string))
    } else {
        None
    }
}

#[rocket::get("/temperature/history")]
fn temperature_history() -> RawJson<String> {
    let buffer = DATA_BUFFER.read().unwrap().to_vec();
    RawJson(json!({"values": buffer}).to_string())
}

#[rocket::get("/temperature/history/<size>")]
fn temperature_history_size(size: usize) -> RawJson<String> {
    let buffer = DATA_BUFFER.read().unwrap();
    let min_size = size.min(buffer.len());

    let values = buffer.iter().rev().take(min_size).collect::<Vec<_>>();
    RawJson(json!({"values": values}).to_string())
}

#[rocket::launch]
fn rocket() -> _ {
    // this crashes the program when reading thread crashes
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        orig_hook(info);
        std::process::exit(1);
    }));

    std::thread::spawn(|| {
        let ports = serialport::available_ports().unwrap();
        let port_info = ports
            .get(0)
            .ok_or("Cannot find temperature sensor")
            .unwrap();
        let port = serialport::new(&port_info.port_name, ESP_BAUD_RATE)
            .timeout(Duration::from_secs(10))
            .open()
            .unwrap();
        let reader = BufReader::new(port);

        let mut counter = 0;
        reader.lines().for_each(|line| {
            if counter % 60 == 0 {
                let line = line.expect("Failed to read line from sensor");
                let mut buffer = DATA_BUFFER.write().unwrap();
                let mut temperature: Temperature =
                    serde_json::from_str(&line).expect("Invalid JSON from sensor");
                temperature.timestamp = chrono::Utc::now();
                buffer.push(temperature);
            }

            counter = (counter + 1) % 60;
        });

        // this crashes the service when the sensor is disconnected
        panic!("temperature sensor disconnected");
    });

    let config = rocket::Config::figment()
        .merge(("port", 9000))
        .merge(("address", "127.0.0.1"));

    rocket::custom(config)
        .mount(
            "/",
            rocket::routes![temperature, temperature_history, temperature_history_size],
        )
        .attach(cors::CORS)
}
