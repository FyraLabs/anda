//! Cappy's note:
//! To anyone who's looking at this crate, I've wrote comments in the code to explain the architecture of the database, and how it
//! should work.
//! Due to our developers being conflicted over the architecture of this project, I've decided to write comprehensive comments to explain
//! what each table does.
//! The other migrations shouldn't contain these explanations, and comments should be added for every change in the migrations.
//!
//! You will probably find contents of this document reused in the actual docstrings in the actual server code.
//! If you have any concerns or questions, please contact Lleyton from Fyra Labs, or me (Cappy) directly.
//! Suggestions are welcome.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts

        // Projects are a collection of builds, with some metadata about the project.
        // For example, project `foo` might have builds `bar` and `baz`.
        // This is for organizational purposes.
        // The project `foo` is the parent of the build `bar` and `baz`. The functionality here is similar to a "Package" in Koji.
        // A project need not be a package, but it can also be ISO, or OSTree composes, or anything else that can be grouped into a project.
        manager
            .create_table(
                Table::create()
                    .table(Project::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Project::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Project::Name).string().not_null())
                    .col(ColumnDef::new(Project::Description).string())
                    .to_owned(),
            )
            .await?;

        // Users are people who will be using this system.
        // Pretty self-explanatory.
        // Users will probably have an API key that allows them to access the system, and a username that can be read by humans
        // We are still going to use UUIDs internally though, so we can use that as the primary key.
        manager
            .create_table(
                Table::create()
                    .table(User::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(User::Id).uuid().not_null().primary_key())
                    .to_owned(),
            )
            .await?;

        // Composes are a collection of artifacts that are compiled together to make a final release.
        // You cobble together a list of project, and the target to compose for, and put them into the compose.
        // The composition process starts out as you tag builds for a target, and then these builds' artifacts are added to the compose
        // These artifacts will then be organized into a tree, (RPMs compiled into repos using `createrepo`) and other files are then organized.
        // The compose is then published to the public repository, and the compose is available for public consumption.
        // This includes different artifacts such as RPMs, ISOs, OSTree refs, all combined into one massive repository.
        // Example: compose `fedora-36` will contain Yum repos for Fedora 36, an actual OSTree ref for our custom Fedora 36, and links to
        // Disk images and OCI images for Fedora 36.
        // This is similar to Pungi's `compose` concept.

        // There should be temporary composes that are continually being updated as new builds are tagged.
        // This is for development where builds can be released without the need for a new compose, allowing you to quickly rebuild new projects with
        // updated dependencies.
        manager
            .create_table(
                Table::create()
                    .table(Compose::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Compose::Id).uuid().not_null().primary_key())
                    .col(
                        // TODO: Lea probably thinks everything is OSTree, Composes are *not* just OSTree refs.
                        // So refs will probably just be the human-readable ID for the artifact.
                        ColumnDef::new(Compose::Ref).string(),
                    )
                    .col(ColumnDef::new(Compose::ProjectId).uuid().not_null())
                    .col(ColumnDef::new(Compose::Timestamp).timestamp_with_time_zone().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-compose-projectid-to-project-id")
                            .from(Compose::Table, Compose::ProjectId)
                            .to(Project::Table, Project::Id),
                    )
                    // Collection of Builds that are part of this compose
                    .to_owned(),
            )
            .await?;

        // Build targets are where builds are tagged for.
        // This will be used to determine which builds to add into a compose.
        // For each project, there should be only one one newest build tagged for each target.
        // this means that project `foo` might have builds `bar` and `baz`, and tagged to `fedora-36` and `fedora-37`.
        // The build `bar` will be tagged for `fedora-36`, and the build `baz` will be tagged for `fedora-37`.
        // but older builds `a` and `b` will not be tagged for any target.

        // This should work similarly to how tags are used in Koji.
        manager
            .create_table(
                Table::create()
                    .table(Target::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Target::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Target::Name).string().not_null())
                    .col(
                        // Allow to be nullable because sometimes you gotta bootstrap a project without an image
                        ColumnDef::new(Target::Image).string(),
                    )
                    .col(
                        // TODO: Make this a string?
                        ColumnDef::new(Target::Arch).integer().not_null(),
                    )
                    /* .col(
                        // Why do you need a project id for a target?
                        ColumnDef::new(Target::ProjectId).uuid().not_null()
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-arch-projectid-to-project-id")
                            .from(Target::Table, Target::ProjectId)
                            .to(Project::Table, Project::Id),
                    ) */
                    .to_owned(),
            )
            .await?;

        // Builds are well, builds.
        // They are tasks that are run to produce an artifact.
        // Multiple artifacts can be produced from a single build.
        // For example, a build might produce multiple RPM packages, and some logs might be produced.
        // These artifacts are then added to the server's artifact registry and uploaded to the S3 bucket (or filesystem).
        // The build is then marked as complete, and the builds now can be tagged for a target.

        // Composes will then look for all builds tagged to a certain target, and add the artifacts to the compose.
        manager
            .create_table(
                Table::create()
                    .table(Build::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Build::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Build::Worker).uuid().not_null())
                    .col(ColumnDef::new(Build::Status).integer().not_null())
                    .col(ColumnDef::new(Build::TargetId).uuid())
                    .col(
                        ColumnDef::new(Build::ProjectId).uuid(), // Project ID can be null, if the build is not associated with a project
                                                                 // Why? So we can let users create test builds without a project.
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-build-projectid-to-project-id")
                            .from(Build::Table, Build::ProjectId)
                            .to(Project::Table, Project::Id),
                    )
                    .col(ColumnDef::new(Build::Timestamp).timestamp_with_time_zone().not_null())
                    // I'm adding this back, but this time this will be nullable.
                    .col(ColumnDef::new(Build::ComposeId).uuid())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-build-targetid-to-target-id")
                            .from(Build::Table, Build::TargetId)
                            .to(Target::Table, Target::Id),
                    )
                    .to_owned(),
            )
            .await?;

        // Artifacts are what are produced by builds.
        // They are in the form of a file, or a reference to an image or a reference to a repository.
        // This is a registry of all the artifacts that have been produced by builds.
        // Each of them will contain a UUID, the file name, and a path to the file.
        manager
            .create_table(
                Table::create()
                    .table(Artifact::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Artifact::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Artifact::Name).string().not_null())
                    .col(ColumnDef::new(Artifact::Url).string().not_null())
                    .col(ColumnDef::new(Artifact::BuildId).uuid().not_null())
                    .col(ColumnDef::new(Artifact::Timestamp).timestamp_with_time_zone().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-artifact-buildid-to-build-id")
                            .from(Artifact::Table, Artifact::BuildId)
                            .to(Build::Table, Build::Id),
                    )
                    // TODO: Implement artifact types as an enum.
                    // Planned types will include RPM. ISO. OSTree, OCI, Disk, logs and generic archives.
                    // This will allow us to have a single table for all artifacts.
                    // For artifacts that are regular files, this should be stored as a reference link to the file on our S3 bucket in `url`.
                    // For OSTree refs and OCI, this should be stored as the OSTree or OCI ref.
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Artifact::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Build::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Target::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Compose::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(User::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Project::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
pub enum Project {
    Table,
    Id,
    Name,
    Description,
    Summary,
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
pub(crate) enum Target {
    Table,
    Id,
    Name,
    Image,
    Arch,
}

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

#[derive(Iden)]
enum Artifact {
    Table,
    BuildId,
    Id,
    Name,
    Url,
    Timestamp,
}
