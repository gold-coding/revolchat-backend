use crate::util::variables::MONGO_URI;

use mongodb::sync::{Client, Collection, Database};
use once_cell::sync::OnceCell;

static DBCONN: OnceCell<Client> = OnceCell::new();

pub fn connect() {
    let client = Client::with_uri_str(&MONGO_URI).expect("Failed to init db connection.");

    DBCONN.set(client).unwrap();
    migrations::run_migrations();
}

pub fn get_connection() -> &'static Client {
    DBCONN.get().unwrap()
}

pub fn get_db() -> Database {
    get_connection().database("revolt")
}

pub fn get_collection(collection: &str) -> Collection {
    get_db().collection(collection)
}

pub mod migrations;

pub mod channel;
pub mod guild;
pub mod message;
pub mod mutual;
pub mod permissions;
pub mod user;

pub use permissions::*;
