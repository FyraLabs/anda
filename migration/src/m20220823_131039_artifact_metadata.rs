use sea_orm_migration::prelude::*;
use sea_orm_migration::prelude::MigrationTrait;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add JSON metadata for artefacts
        // definitions are defined somewhere else
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Alias::new("artifact"))
                    .add_column(ColumnDef::new(Alias::new("metadata")).json_binary())
                    .to_owned(),
            ).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts

        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Alias::new("artifact"))
                    .drop_column(Alias::new("metadata"))
                    .to_owned(),
            ).await
    }
}