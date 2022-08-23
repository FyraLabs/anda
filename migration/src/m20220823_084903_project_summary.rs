use sea_orm_migration::prelude::*;
use crate::m20220101_000001_create_table::{Project};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Summary column

        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Project::Table)
                    .add_column(ColumnDef::new(Project::Summary).text())
                    .to_owned(),
            ).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {

        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Project::Table)
                    .drop_column(Alias::new("summary"))
                    .to_owned(),
            ).await
    }
}
