use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts
        manager
            .create_table(
                Table::create()
                    .table(Project::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Project::Id).uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Project::Name).string().not_null())
                    .col(ColumnDef::new(Project::Description).string().not_null())
                    .to_owned(),
            )
            .await?;

        manager.create_table(
            Table::create()
            .table(User::Table)
            .if_not_exists()
            .col(
                ColumnDef::new(User::Id).uuid()
                    .not_null()
                    .primary_key(),
            )
            .to_owned(),
            )
            .await?;

            manager.create_table(
                Table::create()
                .table(Compose::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Compose::Id).uuid()
                        .not_null()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(Compose::Ref).string().not_null()
                )
                .col(
                    ColumnDef::new(Compose::ProjectId).uuid().not_null()
                )
                .col(
                    ColumnDef::new(Compose::Timestamp).timestamp().not_null()
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk-compose-projectid-to-project-id")
                        .from(Compose::Table, Compose::ProjectId)
                        .to(Project::Table, Project::Id),
                )
                .to_owned()
            ).await?;

            manager.create_table(
                Table::create()
                .table(Target::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Target::Id).uuid()
                        .not_null()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(Target::Name).string().not_null()
                )
                .col(
                    ColumnDef::new(Target::Image).string().not_null()
                )
                .col(
                    ColumnDef::new(Target::Arch).integer().not_null()
                )
                .col(
                    ColumnDef::new(Target::ProjectId).uuid().not_null()
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk-arch-projectid-to-project-id")
                        .from(Target::Table, Target::ProjectId)
                        .to(Project::Table, Project::Id),
                )
                .to_owned()
            ).await?;

            manager.create_table(
                Table::create()
                .table(Build::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Build::Id).uuid()
                        .not_null()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(Build::Worker).uuid().not_null(),
                )
                .col(
                    ColumnDef::new(Build::Status).integer().not_null(),
                )
                .col(
                    ColumnDef::new(Build::TargetId).uuid().not_null(),
                )
                .col(
                    ColumnDef::new(Build::ComposeId).uuid().not_null(),
                )
                .col(
                    ColumnDef::new(Build::Timestamp).timestamp().not_null()
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk-build-targetid-to-target-id")
                        .from(Build::Table, Build::TargetId)
                        .to(Target::Table, Target::Id),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk-build-composeid-to-compose-id")
                        .from(Build::Table, Build::ComposeId)
                        .to(Compose::Table, Compose::Id),
                )
                .to_owned()
            ).await?;

            manager.create_table(
                Table::create()
                .table(Artifact::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Artifact::Id).uuid()
                        .not_null()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(Artifact::Name).string().not_null(),
                )
                .col(
                    ColumnDef::new(Artifact::Url).string().not_null(),
                )
                .col(
                    ColumnDef::new(Artifact::BuildId).uuid().not_null(),
                )
                .col(
                    ColumnDef::new(Artifact::Timestamp).timestamp().not_null()
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk-artifact-buildid-to-build-id")
                        .from(Artifact::Table, Artifact::BuildId)
                        .to(Build::Table, Build::Id),
                )
                .to_owned()
            ).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Project::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(User::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Compose::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Target::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Build::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Artifact::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum Project {
    Table,
    Id,
    Name,
    Description,
}

#[derive(Iden)]
enum User {
    Table,
    Id,
}

#[derive(Iden)]
enum Compose {
    Table,
    Id,
    Ref,
    ProjectId,
    Timestamp,
}

#[derive(Iden)]
enum Target {
    Table,
    Id,
    Name,
    Image,
    Arch,
    ProjectId,
}

#[derive(Iden)]
enum Build {
    Table,
    Id,
    ComposeId,
    TargetId,
    Status,
    Worker,
    Timestamp,
}

#[derive(Iden)]
enum Artifact {
    Table,
    BuildId,
    Id,
    Name,
    Url,
    Timestamp,
}
