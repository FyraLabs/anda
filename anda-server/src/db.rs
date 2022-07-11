//! Database connection code for Anda API server
//! This piece of code exists because we do need a global database connection object that Rocket can manage on startup,
//! but also available to other parts of the code that does not use Rocket's state API (Internal processing).


use sea_orm::*;

use dotenv::dotenv;
use std::env;
use async_once_cell::OnceCell;


pub struct DbPool {
    pub conn: DatabaseConnection,
}

static DB: OnceCell<DatabaseConnection> = OnceCell::new();


impl DbPool {

    pub async fn new() -> Result<DatabaseConnection, DbErr> {
        dotenv().ok();
        let url = env::var("DATABASE_URL").unwrap();
        let db = Database::connect(&url).await?;
        Ok(db)
    }

    pub async fn get() -> &'static DatabaseConnection {
        DB.get_or_init(async {
            DbPool::new().await.unwrap()
        }).await
    }
}

pub(crate) async fn setup_db() -> Result<DatabaseConnection, DbErr> {
    Ok(DbPool::get().await.to_owned())
}