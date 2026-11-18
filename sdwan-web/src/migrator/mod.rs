use sea_orm_migration::prelude::*;

mod m20241029_000001_init;
mod m20260403_000002_scope_network_config_unique;
mod m20260421_000003_add_network_config_source;
mod m20261116_000004_add_app_config;
mod m20261117_000005_add_user_admin_fields;
mod m20261118_000006_add_user_client_limit;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20241029_000001_init::Migration),
            Box::new(m20260403_000002_scope_network_config_unique::Migration),
            Box::new(m20260421_000003_add_network_config_source::Migration),
            Box::new(m20261116_000004_add_app_config::Migration),
            Box::new(m20261117_000005_add_user_admin_fields::Migration),
            Box::new(m20261118_000006_add_user_client_limit::Migration),
        ]
    }
}
