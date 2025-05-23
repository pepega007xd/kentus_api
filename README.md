# Kentus.eu api

Basically what is at [api.kentus.eu](https://api.kentus.eu).

## Setup
- Create the DB: `sqlite3 temperature.sqlite < migrations/20240406154226_create_temperature_table.sql`
- Set the DB path `DATABASE_URL=sqlite:///srv/kentusapi/temperature.sqlite` into `.env` file in the current directory
- Build: `cargo build --release`
