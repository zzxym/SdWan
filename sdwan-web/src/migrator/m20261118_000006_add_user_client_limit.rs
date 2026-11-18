use sea_orm_migration::prelude::*;

/// 新增用户客户端数量上限字段 `users.client_limit`。0 表示不限制，默认 2。
pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261118_000006_add_user_client_limit"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared(
            r#"
            ALTER TABLE users ADD COLUMN client_limit INTEGER NOT NULL DEFAULT 2;
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("ALTER TABLE users DROP COLUMN client_limit;")
            .await?;

        Ok(())
    }
}
