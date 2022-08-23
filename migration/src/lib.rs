pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_table;
mod m20220721_121335_update_schemas;
mod m20220721_144407_remove_workers;
mod m20220823_084903_project_summary;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_table::Migration),
            Box::new(m20220721_121335_update_schemas::Migration),
            Box::new(m20220721_144407_remove_workers::Migration),
            Box::new(m20220823_084903_project_summary::Migration),
        ]
    }
}
