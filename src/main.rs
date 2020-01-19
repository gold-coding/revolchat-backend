#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use] extern crate rocket;
#[macro_use] extern crate rocket_contrib;

pub mod database;
pub mod routes;
pub mod email;
pub mod auth;

use dotenv;

fn main() {
	dotenv::dotenv().ok();
	database::connect();
	email::connect();
	email::send("me@insrt.uk", "test", "test email");

	//routes::mount(rocket::ignite()).launch();
}
