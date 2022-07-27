use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(Iden)]
pub(crate) enum Build {
    Table,
    Id,
    ProjectId,
    ComposeId,
    TargetId,
    Status,
    Worker,
    Timestamp,
    BuildType,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Build::Table)
                    .drop_column(Alias::new("worker"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Build::Table)
                    .add_column(ColumnDef::new(Build::Worker).uuid().not_null())
                    .to_owned(),
            )
            .await
    }
}
