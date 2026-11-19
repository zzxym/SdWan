use std::collections::HashSet;

use async_trait::async_trait;
use axum_login::{AuthUser, AuthnBackend, AuthzBackend, UserId};
use password_auth::verify_password;
use sea_orm::{
    ColumnTrait, EntityTrait, FromQueryResult, IntoActiveModel, JoinType, PaginatorTrait,
    QueryFilter, QuerySelect as _, RelationTrait, Set,
};
use serde::{Deserialize, Serialize};
use tokio::task;

use crate::db::{self, entity};

#[derive(Clone, Serialize, Deserialize)]
pub struct User {
    pub(crate) db_user: entity::users::Model,
    pub tokens: Vec<String>,
}

// Here we've implemented `Debug` manually to avoid accidentally logging the
// password hash.
impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.db_user.id)
            .field("username", &self.db_user.username)
            .field("password", &"[redacted]")
            .finish()
    }
}

impl AuthUser for User {
    type Id = i32;

    fn id(&self) -> Self::Id {
        self.db_user.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.db_user.password.as_bytes() // We use the password hash as the auth
        // hash--what this means
        // is when the user changes their password the
        // auth session becomes invalid.
    }
}

// This allows us to extract the authentication fields from forms. We use this
// to authenticate requests with the backend.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegisterNewUser {
    pub credentials: Credentials,
    pub captcha: String,
    /// 验证码标识，由 `/api/v1/auth/captcha_info` 返回。
    ///
    /// 携带该字段时验证码校验不依赖 session cookie；为空时回退到基于 session 的校验
    /// （兼容旧客户端）。
    #[serde(default)]
    pub captcha_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChangePassword {
    pub new_password: String,
}

#[derive(Debug, Clone)]
pub struct Backend {
    db: db::Db,
}

impl Backend {
    pub fn new(db: db::Db) -> Self {
        Self { db }
    }

    pub fn db(&self) -> &db::Db {
        &self.db
    }

    pub async fn register_new_user(&self, new_user: &RegisterNewUser) -> anyhow::Result<()> {
        let hashed_password = password_auth::generate_hash(new_user.credentials.password.as_str());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let expire_at = Some(now + 30 * 24 * 3600);
        self.db
            .create_user_and_join_users_group(
                &new_user.credentials.username,
                hashed_password,
                expire_at,
            )
            .await?;
        Ok(())
    }

    /// Find a user by username, or auto-create one for OIDC-authenticated users.
    ///
    /// Unlike the heartbeat auto-creation path (controlled by `allow_auto_create_user`),
    /// OIDC users are always provisioned automatically because their identity has already
    /// been verified by a trusted external Identity Provider (IdP).
    pub async fn find_or_create_oidc_user(&self, username: &str) -> anyhow::Result<User> {
        use entity::users;

        // Try to find an existing user first.
        if let Some(db_user) = users::Entity::find()
            .filter(users::Column::Username.eq(username))
            .one(self.db.orm_db())
            .await?
        {
            return Ok(User {
                tokens: vec![db_user.username.clone()],
                db_user,
            });
        }

        // User not found – auto-provision a local account backed by the IdP identity.
        let db_user = self.db.auto_create_user(username).await?;
        tracing::info!("Auto-provisioned OIDC user '{username}'");
        Ok(User {
            tokens: vec![db_user.username.clone()],
            db_user,
        })
    }

    pub async fn change_password(
        &self,
        id: <User as AuthUser>::Id,
        req: &ChangePassword,
    ) -> anyhow::Result<()> {
        let hashed_password = password_auth::generate_hash(req.new_password.as_str());

        use entity::users;

        let mut user = users::Entity::find_by_id(id)
            .one(self.db.orm_db())
            .await?
            .ok_or(anyhow::anyhow!("User not found"))?
            .into_active_model();
        user.password = Set(hashed_password.clone());

        entity::users::Entity::update(user)
            .exec(self.db.orm_db())
            .await?;

        Ok(())
    }

    /// 判断用户是否属于 `admins` 组（即运营管理员）。
    pub async fn is_admin(&self, id: <User as AuthUser>::Id) -> Result<bool, Error> {
        use entity::{groups, users_groups};

        let count = users_groups::Entity::find()
            .join(JoinType::InnerJoin, users_groups::Relation::Groups.def())
            .filter(users_groups::Column::UserId.eq(id))
            .filter(groups::Column::Name.eq("admins"))
            .count(self.db.orm_db())
            .await?;

        Ok(count > 0)
    }

    /// 判断账号当前是否可用：未被禁用且未超过使用期限。
    pub async fn is_user_usable(&self, id: <User as AuthUser>::Id) -> Result<bool, Error> {
        use entity::users;

        let Some(user) = users::Entity::find_by_id(id).one(self.db.orm_db()).await? else {
            return Ok(false);
        };

        Ok(user.is_usable())
    }
}

/// 账号可用性判定：`enabled` 为真，且未设置过期的账号仍在有效期内。
pub trait UserUsable {
    fn is_usable(&self) -> bool;
    fn is_expired(&self) -> bool;
}

impl UserUsable for entity::users::Model {
    fn is_expired(&self) -> bool {
        match self.expire_at {
            Some(expire_at) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                now >= expire_at
            }
            None => false,
        }
    }

    fn is_usable(&self) -> bool {
        self.enabled && !self.is_expired()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sqlx(#[from] sea_orm::DbErr),

    #[error(transparent)]
    TaskJoin(#[from] task::JoinError),
}

#[async_trait]
impl AuthnBackend for Backend {
    type User = User;
    type Credentials = Credentials;
    type Error = Error;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let user = entity::users::Entity::find()
            .filter(entity::users::Column::Username.eq(creds.username))
            .one(self.db.orm_db())
            .await?;
        task::spawn_blocking(|| {
            // We're using password-based authentication--this works by comparing our form
            // input with an argon2 password hash.
            Ok(user
                .filter(|user| verify_password(creds.password, &user.password).is_ok())
                // 被禁用或已过期的账号不允许登录。
                .filter(|user| user.is_usable())
                .map(|user| User {
                    db_user: user.clone(),
                    tokens: vec![user.username.clone()],
                }))
        })
        .await?
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        let mut user = entity::users::Entity::find()
            .filter(entity::users::Column::Id.eq(*user_id))
            .one(self.db.orm_db())
            .await?;

        if let Some(u) = &mut user {
            let mut user = User {
                db_user: u.clone(),
                tokens: vec![],
            };
            // username is a token
            user.tokens.push(u.username.clone());
            Ok(Some(user))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, FromQueryResult)]
pub struct Permission {
    pub name: String,
}

impl From<&str> for Permission {
    fn from(name: &str) -> Self {
        Permission {
            name: name.to_string(),
        }
    }
}

#[async_trait]
impl AuthzBackend for Backend {
    type Permission = Permission;

    async fn get_group_permissions(
        &self,
        user: &Self::User,
    ) -> Result<HashSet<Self::Permission>, Self::Error> {
        let permissions = entity::users::Entity::find()
            .column_as(entity::permissions::Column::Name, "name")
            .join(
                JoinType::LeftJoin,
                entity::users::Relation::UsersGroups.def(),
            )
            .join(
                JoinType::LeftJoin,
                entity::users_groups::Relation::Groups.def(),
            )
            .join(
                JoinType::LeftJoin,
                entity::groups::Relation::GroupsPermissions.def(),
            )
            .join(
                JoinType::LeftJoin,
                entity::groups_permissions::Relation::Permissions.def(),
            )
            // 必须按当前用户过滤：否则会返回全库所有用户的权限并集，
            // 导致任意登录用户都被视为拥有管理员权限。
            .filter(entity::users::Column::Id.eq(user.id()))
            .into_model::<Self::Permission>()
            .all(self.db.orm_db())
            .await?;

        Ok(permissions.into_iter().collect())
    }
}

// We use a type alias for convenience.
//
// Note that we've supplied our concrete backend here.
pub type AuthSession = axum_login::AuthSession<Backend>;

#[cfg(test)]
mod tests {
    use axum_login::AuthzBackend;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    use super::*;
    use crate::db::entity::{groups, users};

    /// 迁移应当：创建默认管理员 xlxxadmin、禁用内置 admin 账号。
    #[tokio::test]
    async fn test_migration_seeds_default_admin_and_disables_builtin_admin() {
        let db = crate::db::Db::memory_db().await;
        let backend = Backend::new(db);

        let admin_user = users::Entity::find()
            .filter(users::Column::Username.eq("xlxxadmin"))
            .one(backend.db().orm_db())
            .await
            .unwrap()
            .expect("xlxxadmin should be created by migration");
        assert!(admin_user.enabled, "default admin must be enabled");
        assert!(
            backend.is_admin(admin_user.id).await.unwrap(),
            "xlxxadmin must belong to the admins group"
        );

        let builtin_admin = users::Entity::find()
            .filter(users::Column::Username.eq("admin"))
            .one(backend.db().orm_db())
            .await
            .unwrap()
            .expect("builtin admin user should exist");
        assert!(
            !builtin_admin.enabled,
            "builtin admin account must be disabled by default"
        );
    }

    /// 权限查询必须按用户过滤，普通用户不应拿到管理员权限。
    #[tokio::test]
    async fn test_group_permissions_are_scoped_to_the_user() {
        let db = crate::db::Db::memory_db().await;
        let backend = Backend::new(db);

        let normal_id = backend
            .db()
            .get_user_id("user")
            .await
            .unwrap()
            .expect("seeded user should exist");
        let admin_id = backend
            .db()
            .get_user_id("xlxxadmin")
            .await
            .unwrap()
            .expect("seeded admin should exist");

        let normal_user = User {
            db_user: users::Entity::find_by_id(normal_id)
                .one(backend.db().orm_db())
                .await
                .unwrap()
                .unwrap(),
            tokens: vec![],
        };
        let admin_user = User {
            db_user: users::Entity::find_by_id(admin_id)
                .one(backend.db().orm_db())
                .await
                .unwrap()
                .unwrap(),
            tokens: vec![],
        };

        assert!(
            !backend.is_admin(normal_id).await.unwrap(),
            "normal user must not be an admin"
        );
        assert!(backend.is_admin(admin_id).await.unwrap());

        // 普通用户的权限集合不应包含 admins 组带来的全部权限。
        let normal_perms = backend.get_group_permissions(&normal_user).await.unwrap();
        assert!(
            !normal_perms.contains(&Permission {
                name: "sessions".to_string()
            }),
            "normal user must not inherit admins group permissions"
        );

        // 管理员应能拿到权限。
        let admin_perms = backend.get_group_permissions(&admin_user).await.unwrap();
        assert!(!admin_perms.is_empty(), "admin should have permissions");
    }

    /// 禁用 / 过期的账号不可用。
    #[tokio::test]
    async fn test_disabled_and_expired_users_are_not_usable() {
        let mut user = users::Model {
            id: 1,
            username: "u".to_string(),
            password: "p".to_string(),
            enabled: true,
            expire_at: None,
            client_limit: 2,
        };
        assert!(user.is_usable());

        user.enabled = false;
        assert!(!user.is_usable(), "disabled user must not be usable");

        user.enabled = true;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        user.expire_at = Some(now - 1);
        assert!(user.is_expired(), "past expiration must be expired");
        assert!(!user.is_usable(), "expired user must not be usable");

        user.expire_at = Some(now + 3600);
        assert!(!user.is_expired());
        assert!(user.is_usable());
    }

    /// admins 组必须存在（迁移种子数据）。
    #[tokio::test]
    async fn test_admins_group_exists() {
        let db = crate::db::Db::memory_db().await;
        let admins = groups::Entity::find()
            .filter(groups::Column::Name.eq("admins"))
            .one(db.orm_db())
            .await
            .unwrap();
        assert!(admins.is_some(), "admins group must be seeded");
    }
}
