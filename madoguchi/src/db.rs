use rocket_db_pools::{sqlx::PgPool, Database};

#[derive(Database)]
#[database("madoguchi")]
pub struct Madoguchi(PgPool);

pub struct Repo {
	pub name: String,
	pub link: String,
	pub gh: String,
}

pub struct Pkg {
	pub name: String,
	pub repo: Repo,
	pub verl: String,
	pub arch: String,
	pub dirs: String,
	pub build: Option<i64>,
}

pub struct Build {
	pub id: i64,
	pub epoch: sqlx::types::chrono::NaiveDateTime,
	pub pname: String,
	pub pverl: String,
	pub parch: String,
	pub repo: Repo,
	pub link: String,
}
