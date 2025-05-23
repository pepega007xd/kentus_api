CREATE TABLE IF NOT EXISTS outdoor_temperature (
    record_timestamp DATETIME PRIMARY KEY NOT NULL DEFAULT CURRENT_TIMESTAMP,
    temperature REAL NOT NULL,
    pressure REAL NOT NULL,
    humidity REAL NOT NULL
);
