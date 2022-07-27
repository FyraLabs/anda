//! Database connection code for Anda API server
//! This piece of code exists because we do need a global database connection object that Rocket can manage on startup,
//! but also available to other parts of the code that does not use Rocket's state API (Internal processing).

use async_once_cell::OnceCell;
use dotenv::dotenv;
use sea_orm::*;
use sea_orm_rocket::{rocket::figment::Figment, Config, Database};
use std::env;

pub struct DbPool {
    pub conn: sea_orm::DatabaseConnection,
}

static DB: OnceCell<DatabaseConnection> = OnceCell::new();

impl DbPool {
    pub async fn new() -> Result<DatabaseConnection, DbErr> {
        dotenv().ok();
        let url = env::var("DATABASE_URL").unwrap_or_else(|_| panic!("$DATABSE_URL not set"));
        let mut opts = ConnectOptions::new(url);
        opts.max_connections(100)
            .min_connections(5)
            .sqlx_logging(true);

        let db = sea_orm::Database::connect(opts).await?;
        Ok(db)
    }

    pub async fn get() -> &'static DatabaseConnection {
        DB.get_or_init(async { DbPool::new().await.unwrap() }).await
    }
    pub async fn init() -> Result<Self, DbErr> {
        Ok(DbPool {
            conn: DbPool::new().await?,
        })
    }
}

#[derive(Database)]
#[database("anda")]
pub struct Db(DbPool);

#[async_trait]
impl sea_orm_rocket::Pool for DbPool {
    type Error = sea_orm::DbErr;

    type Connection = sea_orm::DatabaseConnection;

    async fn init(_figment: &Figment) -> Result<Self, Self::Error> {
        let conn = DbPool::get().await.to_owned();

        Ok(DbPool { conn })
    }

    fn borrow(&self) -> &Self::Connection {
        &self.conn
    }
}
