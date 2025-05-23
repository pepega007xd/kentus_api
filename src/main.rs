use chrono::prelude::*;
use dotenvy_macro::dotenv;
use rocket::futures::{future, StreamExt};
use rocket::serde::json::Json;
use rocket::tokio::sync::OnceCell;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

mod cors;

static DATABASE: OnceCell<SqlitePool> = OnceCell::const_new();

#[derive(Serialize, Deserialize)]
struct Temperature {
    temperature: f64,
    humidity: f64,
    #[serde(skip_deserializing)]
    record_timestamp: NaiveDateTime,
}

#[derive(Serialize, Deserialize)]
struct OutdoorTemperature {
    temperature: f64,
    pressure: f64,
    humidity: f64,
    #[serde(skip_deserializing)]
    record_timestamp: NaiveDateTime,
}

#[rocket::post("/indoor_sensor", data = "<measurements>")]
async fn indoor_sensor(measurements: Json<Temperature>) -> Option<()> {
    sqlx::query!(
        "INSERT INTO temperature (temperature, humidity)
                VALUES (?1, ?2)",
        measurements.temperature,
        measurements.humidity,
    )
    .execute(DATABASE.get()?)
    .await
    .ok()?;

    Some(())
}

#[rocket::post("/outdoor_sensor", data = "<measurements>")]
async fn outdoor_sensor(measurements: Json<OutdoorTemperature>) -> Option<()> {
    sqlx::query!(
        "INSERT INTO outdoor_temperature (temperature, pressure, humidity)
                VALUES (?1, ?2, ?3)",
        measurements.temperature,
        measurements.pressure,
        measurements.humidity,
    )
    .execute(DATABASE.get()?)
    .await
    .ok()?;

    Some(())
}

#[rocket::get("/temperature")]
async fn temperature() -> Option<Json<Temperature>> {
    let latest_temp: Temperature = sqlx::query_as!(
        Temperature,
        "SELECT * FROM temperature ORDER BY record_timestamp DESC"
    )
    .fetch_one(DATABASE.get()?)
    .await
    .ok()?
    .into();

    Some(Json(latest_temp))
}

#[rocket::get("/temperature/history?<count>")]
async fn temperature_history(count: Option<i64>) -> Option<Json<Value>> {
    let count = count.unwrap_or(24 * 60);
    let latest_temp = sqlx::query_as!(
        Temperature,
        "SELECT * FROM temperature ORDER BY record_timestamp DESC LIMIT ?1",
        count
    )
    .fetch(DATABASE.get()?)
    .filter_map(|row| future::ready::<Option<Temperature>>(row.map(|row| row.into()).ok()))
    .collect::<Vec<_>>()
    .await;

    Some(Json(json!({"values": latest_temp})))
}

#[rocket::get("/outdoor_temperature")]
async fn outdoor_temperature() -> Option<Json<OutdoorTemperature>> {
    let latest_outdoor_temp = sqlx::query_as!(
        OutdoorTemperature,
        "SELECT * FROM outdoor_temperature ORDER BY record_timestamp DESC"
    )
    .fetch_one(DATABASE.get()?)
    .await
    .ok()?
    .into();

    Some(Json(latest_outdoor_temp))
}

#[rocket::get("/outdoor_temperature/history?<count>")]
async fn outdoor_temperature_history(count: Option<i64>) -> Option<Json<Value>> {
    let count = count.unwrap_or(24 * 60);
    let latest_temp = sqlx::query_as!(
        OutdoorTemperature,
        "SELECT * FROM outdoor_temperature ORDER BY record_timestamp DESC LIMIT ?1",
        count
    )
    .fetch(DATABASE.get()?)
    .filter_map(|row| future::ready::<Option<OutdoorTemperature>>(row.map(|row| row.into()).ok()))
    .collect::<Vec<_>>()
    .await;

    Some(Json(json!({"values": latest_temp})))
}

#[rocket::main]
async fn main() -> anyhow::Result<()> {
    let db = SqlitePoolOptions::new()
        .connect(dotenv!("DATABASE_URL"))
        .await?;

    DATABASE.set(db)?;

    let config = rocket::Config::figment()
        .merge(("port", 8002))
        .merge(("address", "0.0.0.0"));

    rocket::custom(config)
        .mount(
            "/",
            rocket::routes![
                indoor_sensor,
                outdoor_sensor,
                temperature,
                temperature_history,
                outdoor_temperature,
                outdoor_temperature_history,
            ],
        )
        .attach(cors::CORS)
        .launch()
        .await?;

    Ok(())
}
