mod db;

use db::Madoguchi;
use rocket::*;
use rocket_db_pools::{
	sqlx::{Executor, Row, Acquire},
	Connection, Database,
};
use sqlx::{Transaction, Postgres};
use crate::db::Repo;

#[get("/test")]
async fn test(mut db: Connection<Madoguchi>) -> Option<String> {
	let mut t: Transaction<Postgres> = db.begin().await.ok()?;
	sqlx::query!("SELECT * FROM repos")
		.fetch_one(&mut *t).await
		.map(|record| record.name).ok()
}

#[launch]
fn rocket() -> _ {
	rocket::build().attach(Madoguchi::init()).mount("/", routes![test])
}
