use sea_orm_migration::prelude::*;

/// 运营管理能力所需的用户字段与默认管理员账号。
///
/// - `users.enabled`：账号是否启用。内置 `admin` 账号默认禁用（避免默认口令可被直接使用）。
/// - `users.expire_at`：使用期限（Unix 秒）。为空表示永久有效。
/// - `xlxxadmin`：默认管理员账号，加入 `admins` 组。
///
/// 口令约定与现有实现保持一致：前端登录时先对明文做 MD5，服务端再做 argon2 哈希，
/// 因此这里对明文先取 MD5 十六进制串再做 argon2 哈希。
pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261117_000005_add_user_admin_fields"
    }
}

const DEFAULT_ADMIN_USERNAME: &str = "xlxxadmin";
/// 默认管理员初始口令，可通过环境变量 `SDWAN_ADMIN_PASSWORD` 覆盖。
const DEFAULT_ADMIN_PASSWORD_ENV: &str = "SDWAN_ADMIN_PASSWORD";
const DEFAULT_ADMIN_PASSWORD: &str = "xlxxadmin";

/// 计算 `argon2(md5(明文))`，与注册/登录流程使用的口令格式一致。
fn hash_password(plain: &str) -> String {
    use md5::{Digest, Md5};
    let md5_hex = format!("{:x}", Md5::digest(plain.as_bytes()));
    password_auth::generate_hash(&md5_hex)
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 1. 新增字段。SQLite 的 ALTER TABLE ADD COLUMN 不支持带默认值以外的约束，
        //    这里为已有行统一填入默认值（启用 + 永久有效）。
        db.execute_unprepared(
            r#"
            ALTER TABLE users ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1;
            ALTER TABLE users ADD COLUMN expire_at INTEGER NULL;
            "#,
        )
        .await?;

        // 2. 内置 admin 账号默认禁用，避免使用默认口令直接登录。
        db.execute_unprepared("UPDATE users SET enabled = 0 WHERE username = 'admin';")
            .await?;

        // 3. 创建默认管理员账号 xlxxadmin，并加入 admins 组。
        let plain_password = std::env::var(DEFAULT_ADMIN_PASSWORD_ENV)
            .ok()
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| DEFAULT_ADMIN_PASSWORD.to_string());
        let password_hash = hash_password(&plain_password);

        db.execute_unprepared(&format!(
            r#"
            INSERT OR IGNORE INTO users (username, password, enabled, expire_at)
            VALUES ('{DEFAULT_ADMIN_USERNAME}', '{password_hash}', 1, NULL);
            "#,
        ))
        .await?;

        db.execute_unprepared(&format!(
            r#"
            INSERT OR IGNORE INTO users_groups (user_id, group_id)
            SELECT u.id, g.id FROM users u, groups g
            WHERE u.username = '{DEFAULT_ADMIN_USERNAME}' AND g.name = 'admins';
            "#,
        ))
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared(
            r#"
            DELETE FROM users_groups WHERE user_id IN (SELECT id FROM users WHERE username = 'xlxxadmin');
            DELETE FROM users WHERE username = 'xlxxadmin';
            ALTER TABLE users DROP COLUMN expire_at;
            ALTER TABLE users DROP COLUMN enabled;
            "#,
        )
        .await?;

        Ok(())
    }
}
