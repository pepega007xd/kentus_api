# Kentus API

API endpoints at [api.kentus.eu](https://api.kentus.eu).

## Setup

- Set the DB path `DATABASE_URL=sqlite://./temperature.sqlite` into `.env` file in the current directory
- Create the DB: `cargo sqlx database setup`
- Build: `cargo build --release`
