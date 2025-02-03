use anyhow::{bail, Context, Ok};
use chrono::prelude::*;
use dotenvy_macro::dotenv;
use rocket::futures::{future, StreamExt};
use rocket::tokio;
use rocket::{response::content::RawJson, tokio::sync::OnceCell};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::{
    io::{BufRead, BufReader},
    time::Duration,
};

mod cors;

static DATABASE_POOL: OnceCell<SqlitePool> = OnceCell::const_new();

#[derive(Serialize, Deserialize, Clone)]
struct Temperature {
    temperature: f32,
    humidity: f32,
    #[serde(default = "Utc::now")]
    timestamp: DateTime<Utc>,
}

impl From<DbTemperature> for Temperature {
    fn from(db_temp: DbTemperature) -> Self {
        Temperature {
            temperature: db_temp.temperature as f32,
            humidity: db_temp.humidity as f32,
            timestamp: db_temp.record_timestamp.and_utc(),
        }
    }
}

struct DbTemperature {
    temperature: f64,
    humidity: f64,
    record_timestamp: NaiveDateTime,
}

const ESP_BAUD_RATE: u32 = 115200;

#[rocket::get("/temperature")]
async fn temperature() -> Option<RawJson<String>> {
    let latest_temp: Temperature = sqlx::query_as!(
        DbTemperature,
        "SELECT * FROM temperature ORDER BY record_timestamp DESC"
    )
    .fetch_one(DATABASE_POOL.get()?)
    .await
    .ok()?
    .into();

    let json_string = serde_json::to_string(&latest_temp).expect("cannot serialize Temperature");
    Some(RawJson(json_string))
}

#[rocket::get("/temperature/history")]
async fn temperature_history() -> Option<RawJson<String>> {
    temperature_history_size(24 * 60).await // one day of records
}

#[rocket::get("/temperature/history/<size>")]
async fn temperature_history_size(size: usize) -> Option<RawJson<String>> {
    let size = size as i64;
    let latest_temp = sqlx::query_as!(
        DbTemperature,
        "SELECT * FROM temperature ORDER BY record_timestamp DESC LIMIT ?1",
        size
    )
    .fetch(DATABASE_POOL.get()?)
    .filter_map(|row| future::ready::<Option<Temperature>>(row.map(|row| row.into()).ok()))
    .collect::<Vec<_>>()
    .await;

    Some(RawJson(json!({"values": latest_temp}).to_string()))
}

async fn setup_db(database: &OnceCell<SqlitePool>) -> anyhow::Result<()> {
    let db = SqlitePoolOptions::new()
        .connect(dotenv!("DATABASE_URL"))
        .await
        .expect("Cannot connect to database");

    database.set(db)?;
    Ok(())
}

async fn write_data_to_db(database: &OnceCell<SqlitePool>) -> anyhow::Result<()> {
    let ports = serialport::available_ports().unwrap();
    let port_info = ports
        .get(0)
        .ok_or("Cannot find temperature sensor")
        .unwrap();
    let port = serialport::new(&port_info.port_name, ESP_BAUD_RATE)
        .timeout(Duration::from_secs(10))
        .open()
        .expect("Cannot open serial port");
    let reader = BufReader::new(port);

    let mut counter = 0;
    for line in reader.lines() {
        if counter % 60 == 0 {
            let line = line.expect("Failed to read line from sensor");
            let mut temperature: Temperature =
                serde_json::from_str(&line).expect("Invalid JSON from sensor");
            temperature.timestamp = chrono::Utc::now();

            sqlx::query!(
                "INSERT INTO temperature (temperature, humidity)
                VALUES (?1, ?2)",
                temperature.temperature,
                temperature.humidity
            )
            .execute(database.get().context("database connection is created")?)
            .await?;
        }

        counter = (counter + 1) % 60;
    }

    // this crashes the service when the sensor is disconnected
    bail!("temperature sensor disconnected")
}

#[rocket::main]
async fn main() -> anyhow::Result<()> {
    // this crashes the program when reading thread crashes
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        orig_hook(info);
        std::process::exit(1);
    }));

    setup_db(&DATABASE_POOL)
        .await
        .expect("Cannot connect to database");

    tokio::spawn(write_data_to_db(&DATABASE_POOL));

    let config = rocket::Config::figment()
        .merge(("port", 9000))
        .merge(("address", "127.0.0.1"));

    let server_result = rocket::custom(config)
        .mount(
            "/",
            rocket::routes![temperature, temperature_history, temperature_history_size],
        )
        .attach(cors::CORS)
        .launch()
        .await;

    // close connection to the database when gracefully exiting
    // DATABASE_POOL.get().unwrap().close().await;

    server_result?;
    Ok(())
}
