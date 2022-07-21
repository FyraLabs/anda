use crate::m20220101_000001_create_table::{Build, Target};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts

        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Target::Table)
                    .modify_column(ColumnDef::new(Target::Arch).string().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Build::Table)
                    .add_column(ColumnDef::new(Build::BuildType).string())
                    .to_owned(),
            )
            .await?;

        // For each build, set the build type to "generic"

        let builds_query = Query::insert()
            .into_table(Build::Table)
            .columns(vec![Build::BuildType])
            .values(vec!["generic".into()])
            .unwrap()
            .select_from(
                Query::select()
                    .column(Build::BuildType)
                    .and_where(Expr::col(Build::BuildType).is_null())
                    .from(Build::Table)
                    .to_owned(),
            )
            .unwrap()
            .to_owned();

        manager.exec_stmt(builds_query).await?;

        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Build::Table)
                    .add_column(ColumnDef::new(Build::BuildType).string().not_null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts

        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Target::Table)
                    .modify_column(ColumnDef::new(Target::Arch).string())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Build::Table)
                    .drop_column(Build::BuildType)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
