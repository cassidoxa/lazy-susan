pub use sea_orm_migration::prelude::*;

mod m20250516_210859_initial_migration;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20250516_210859_initial_migration::Migration)]
    }
}
