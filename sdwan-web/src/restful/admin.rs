//! 运营管理接口：查看与管控所有用户。
//!
//! 所有接口都要求调用者是 `admins` 组成员（运营管理员）。管理员可以：
//! - 查看全部用户列表
//! - 提权/降权其他用户（加入/移出 `admins` 组）
//! - 启用/禁用账号
//! - 设置账号使用期限
//! - 重置其他用户密码
//! - 为其他用户增删改网络实例
//!
//! 路由挂载在 `/api/v1/admin` 下。

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use axum_login::AuthUser;
use sdwan::launcher::NetworkConfig;
use sdwan::proto::common::Void;
use sdwan::rpc_service::remote_client::RemoteClientManager;
use sea_orm::{
    ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder, QuerySelect as _,
    RelationTrait, Set, TransactionTrait as _,
};

use crate::db::UserIdInDb;

use super::network::{RunNetworkJsonReq, UpdateNetworkStateJsonReq, convert_error};
use super::users::{AuthSession, Backend, UserUsable};
use super::{AppState, AppStateInner, HttpHandleError, convert_db_error, other_error};

/// 组名：`admins` 组成员即为运营管理员。
const ADMIN_GROUP: &str = "admins";

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct AdminUserItem {
    pub id: i32,
    pub username: String,
    /// 是否为运营管理员（属于 `admins` 组）。
    pub is_admin: bool,
    /// 账号是否启用。
    pub enabled: bool,
    /// 使用期限（Unix 秒），为空表示永久有效。
    pub expire_at: Option<i64>,
    /// 是否已过期。
    pub expired: bool,
    /// 该用户名下已注册的客户端（机器）数量。
    pub device_count: usize,
    /// 客户端数量上限。0 表示不限制。
    pub client_limit: i32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ListAdminUsersResp {
    pub users: Vec<AdminUserItem>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SetRoleReq {
    /// true 表示提权为管理员，false 表示取消管理员权限。
    pub is_admin: bool,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SetEnabledReq {
    pub enabled: bool,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SetExpireReq {
    /// 使用期限（Unix 秒）。传 null 表示永久有效。
    pub expire_at: Option<i64>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SetClientLimitReq {
    /// 客户端数量上限。0 表示不限制，必须为非负数。
    pub client_limit: i32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ResetPasswordReq {
    /// 新密码明文，服务端按既有约定（MD5 + argon2）哈希后存储。
    pub new_password: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct DeleteUserReq {
    /// 当前管理员的密码（前端提交 MD5 后的值），用于确认删除操作。
    pub admin_password: String,
}

pub struct AdminApi;

impl AdminApi {
    /// 确认当前会话对应的用户是运营管理员，否则返回 403。
    async fn require_admin(auth_session: &AuthSession) -> Result<UserIdInDb, HttpHandleError> {
        let Some(user) = auth_session.user.as_ref() else {
            return Err((
                StatusCode::UNAUTHORIZED,
                other_error("Not logged in".to_string()).into(),
            ));
        };

        let is_admin = auth_session
            .backend
            .is_admin(user.id())
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    other_error(format!("Failed to check admin permission: {:?}", e)).into(),
                )
            })?;

        if !is_admin {
            return Err((
                StatusCode::FORBIDDEN,
                other_error("Administrator permission required".to_string()).into(),
            ));
        }

        Ok(user.id())
    }

    /// 按用户名查找用户 id。
    async fn find_user_id(
        backend: &Backend,
        username: &str,
    ) -> Result<UserIdInDb, HttpHandleError> {
        backend
            .db()
            .get_user_id(username)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    other_error(format!("DB Error: {:#}", e)).into(),
                )
            })?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    other_error(format!("User '{}' not found", username)).into(),
                )
            })
    }

    /// 列出全部用户及其管理状态。
    async fn handle_list_users(
        auth_session: AuthSession,
        State(client_mgr): AppState,
    ) -> Result<Json<ListAdminUsersResp>, HttpHandleError> {
        Self::require_admin(&auth_session).await?;

        use crate::db::entity::{groups, users, users_groups};

        let db = auth_session.backend.db().orm_db();

        let all_users = users::Entity::find()
            .order_by_asc(users::Column::Id)
            .all(db)
            .await
            .map_err(convert_db_error)?;

        let admin_ids = users_groups::Entity::find()
            .join(
                sea_orm::JoinType::InnerJoin,
                users_groups::Relation::Groups.def(),
            )
            .filter(groups::Column::Name.eq(ADMIN_GROUP))
            .all(db)
            .await
            .map_err(convert_db_error)?
            .into_iter()
            .map(|ug| ug.user_id)
            .collect::<Vec<_>>();

        let mut users_resp = Vec::with_capacity(all_users.len());
        for user in all_users {
            let device_count = client_mgr.list_machine_by_user_id(user.id).await.len();
            users_resp.push(AdminUserItem {
                id: user.id,
                username: user.username.clone(),
                is_admin: admin_ids.contains(&user.id),
                enabled: user.enabled,
                expire_at: user.expire_at,
                expired: user.is_expired(),
                device_count,
                client_limit: user.client_limit,
            });
        }

        Ok(Json(ListAdminUsersResp { users: users_resp }))
    }

    /// 提权 / 降权：把用户加入或移出 `admins` 组。
    async fn handle_set_role(
        auth_session: AuthSession,
        Path(username): Path<String>,
        Json(payload): Json<SetRoleReq>,
    ) -> Result<Json<Void>, HttpHandleError> {
        Self::require_admin(&auth_session).await?;

        use crate::db::entity::{groups, users_groups};

        let backend = auth_session.backend.clone();
        let target_id = Self::find_user_id(&backend, &username).await?;
        let db = backend.db().orm_db();

        let admins_group = groups::Entity::find()
            .filter(groups::Column::Name.eq(ADMIN_GROUP))
            .one(db)
            .await
            .map_err(convert_db_error)?
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    other_error("Admins group not found".to_string()).into(),
                )
            })?;

        let existing = users_groups::Entity::find()
            .filter(users_groups::Column::UserId.eq(target_id))
            .filter(users_groups::Column::GroupId.eq(admins_group.id))
            .one(db)
            .await
            .map_err(convert_db_error)?;

        if payload.is_admin {
            if existing.is_none() {
                users_groups::Entity::insert(users_groups::ActiveModel {
                    user_id: Set(target_id),
                    group_id: Set(admins_group.id),
                    ..Default::default()
                })
                .exec(db)
                .await
                .map_err(convert_db_error)?;
            }
        } else if let Some(existing) = existing {
            users_groups::Entity::delete_by_id(existing.id)
                .exec(db)
                .await
                .map_err(convert_db_error)?;
        }

        Ok(Void::default().into())
    }

    /// 启用 / 禁用账号。
    async fn handle_set_enabled(
        auth_session: AuthSession,
        Path(username): Path<String>,
        Json(payload): Json<SetEnabledReq>,
    ) -> Result<Json<Void>, HttpHandleError> {
        Self::require_admin(&auth_session).await?;

        use crate::db::entity::users;

        let backend = auth_session.backend.clone();
        let target_id = Self::find_user_id(&backend, &username).await?;

        let mut user = users::Entity::find_by_id(target_id)
            .one(backend.db().orm_db())
            .await
            .map_err(convert_db_error)?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    other_error(format!("User '{}' not found", username)).into(),
                )
            })?
            .into_active_model();
        user.enabled = Set(payload.enabled);

        users::Entity::update(user)
            .exec(backend.db().orm_db())
            .await
            .map_err(convert_db_error)?;

        Ok(Void::default().into())
    }

    /// 设置账号使用期限。`expire_at` 为 null 表示永久有效。
    async fn handle_set_expire(
        auth_session: AuthSession,
        Path(username): Path<String>,
        Json(payload): Json<SetExpireReq>,
    ) -> Result<Json<Void>, HttpHandleError> {
        Self::require_admin(&auth_session).await?;

        use crate::db::entity::users;

        let backend = auth_session.backend.clone();
        let target_id = Self::find_user_id(&backend, &username).await?;

        let mut user = users::Entity::find_by_id(target_id)
            .one(backend.db().orm_db())
            .await
            .map_err(convert_db_error)?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    other_error(format!("User '{}' not found", username)).into(),
                )
            })?
            .into_active_model();
        user.expire_at = Set(payload.expire_at);

        users::Entity::update(user)
            .exec(backend.db().orm_db())
            .await
            .map_err(convert_db_error)?;

        Ok(Void::default().into())
    }

    /// 设置账号的授权客户端数量上限。0 表示不限制。
    async fn handle_set_client_limit(
        auth_session: AuthSession,
        Path(username): Path<String>,
        Json(payload): Json<SetClientLimitReq>,
    ) -> Result<Json<Void>, HttpHandleError> {
        Self::require_admin(&auth_session).await?;

        if payload.client_limit < 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                other_error("client_limit must be non-negative".to_string()).into(),
            ));
        }

        use crate::db::entity::users;

        let backend = auth_session.backend.clone();
        let target_id = Self::find_user_id(&backend, &username).await?;

        let mut user = users::Entity::find_by_id(target_id)
            .one(backend.db().orm_db())
            .await
            .map_err(convert_db_error)?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    other_error(format!("User '{}' not found", username)).into(),
                )
            })?
            .into_active_model();
        user.client_limit = Set(payload.client_limit);

        users::Entity::update(user)
            .exec(backend.db().orm_db())
            .await
            .map_err(convert_db_error)?;

        Ok(Void::default().into())
    }

    /// 重置其他用户的密码。
    async fn handle_reset_password(
        auth_session: AuthSession,
        Path(username): Path<String>,
        Json(payload): Json<ResetPasswordReq>,
    ) -> Result<Json<Void>, HttpHandleError> {
        Self::require_admin(&auth_session).await?;

        use crate::restful::users::ChangePassword;

        let backend = auth_session.backend.clone();
        let target_id = Self::find_user_id(&backend, &username).await?;

        backend
            .change_password(
                target_id,
                &ChangePassword {
                    new_password: payload.new_password,
                },
            )
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    other_error(format!("Failed to reset password: {:?}", e)).into(),
                )
            })?;

        Ok(Void::default().into())
    }

    /// 删除用户账号。必须校验当前管理员密码，避免误删。
    async fn handle_delete_user(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path(username): Path<String>,
        Json(payload): Json<DeleteUserReq>,
    ) -> Result<Json<Void>, HttpHandleError> {
        let admin_id = Self::require_admin(&auth_session).await?;

        // 校验当前管理员自己的密码：前端按既有约定提交 MD5 后的值。
        let Some(admin_user) = auth_session.user.as_ref() else {
            return Err((
                StatusCode::UNAUTHORIZED,
                other_error("Not logged in".to_string()).into(),
            ));
        };
        let stored_hash = admin_user.db_user.password.clone();
        let submitted = payload.admin_password.clone();
        let password_ok = tokio::task::spawn_blocking(move || {
            password_auth::verify_password(&submitted, &stored_hash).is_ok()
        })
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                other_error(format!("Failed to verify password: {:?}", e)).into(),
            )
        })?;
        if !password_ok {
            return Err((
                StatusCode::FORBIDDEN,
                other_error("Incorrect administrator password".to_string()).into(),
            ));
        }

        let backend = auth_session.backend.clone();
        let target_id = Self::find_user_id(&backend, &username).await?;

        // 不允许删除当前登录的管理员账号，否则会失去管理入口。
        if target_id == admin_id {
            return Err((
                StatusCode::BAD_REQUEST,
                other_error("Cannot delete the account you are logged in with".to_string()).into(),
            ));
        }

        // 先断开该用户的在线客户端，避免账号删除后旧会话仍能运行网络。
        for (machine_id, _) in client_mgr.list_machines_with_ids(target_id).await {
            client_mgr
                .disconnect_session_by_machine_id(target_id, &machine_id)
                .await;
        }

        use crate::db::entity::{user_running_network_configs as urnc, users, users_groups};

        let db = backend.db().orm_db();
        let txn = db.begin().await.map_err(convert_db_error)?;
        urnc::Entity::delete_many()
            .filter(urnc::Column::UserId.eq(target_id))
            .exec(&txn)
            .await
            .map_err(convert_db_error)?;
        users_groups::Entity::delete_many()
            .filter(users_groups::Column::UserId.eq(target_id))
            .exec(&txn)
            .await
            .map_err(convert_db_error)?;
        users::Entity::delete_by_id(target_id)
            .exec(&txn)
            .await
            .map_err(convert_db_error)?;
        txn.commit().await.map_err(convert_db_error)?;

        Ok(Void::default().into())
    }

    /// 列出某个用户的机器（客户端）。
    async fn handle_list_user_machines(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path(username): Path<String>,
    ) -> Result<Json<serde_json::Value>, HttpHandleError> {
        Self::require_admin(&auth_session).await?;

        let backend = auth_session.backend.clone();
        let target_id = Self::find_user_id(&backend, &username).await?;

        let client_pairs = client_mgr.list_machines_with_ids(target_id).await;
        let mut machines = vec![];
        for (machine_id, client_url) in client_pairs.iter() {
            let session = client_mgr.get_heartbeat_requests(client_url).await;
            let location = client_mgr.get_machine_location(client_url).await;
            machines.push(serde_json::json!({
                "machine_id": machine_id,
                "client_url": client_url.to_string(),
                "info": session,
                "location": location,
            }));
        }

        Ok(Json(serde_json::json!({ "machines": machines })))
    }

    /// 踢出某个用户下的一台在线客户端。
    async fn handle_kick_machine(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path((username, machine_id)): Path<(String, uuid::Uuid)>,
    ) -> Result<Json<serde_json::Value>, HttpHandleError> {
        Self::require_admin(&auth_session).await?;

        let backend = auth_session.backend.clone();
        let target_id = Self::find_user_id(&backend, &username).await?;

        let kicked = client_mgr
            .disconnect_session_by_machine_id(target_id, &machine_id)
            .await;

        Ok(Json(serde_json::json!({ "success": kicked })))
    }

    /// 列出某个用户在某台机器上的网络实例 id。
    async fn handle_list_user_networks(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path((username, machine_id)): Path<(String, uuid::Uuid)>,
    ) -> Result<
        Json<sdwan::rpc_service::remote_client::ListNetworkInstanceIdsJsonResp>,
        HttpHandleError,
    > {
        Self::require_admin(&auth_session).await?;

        let backend = auth_session.backend.clone();
        let target_id = Self::find_user_id(&backend, &username).await?;

        Ok(Json(
            client_mgr
                .handle_list_network_instance_ids((target_id, machine_id))
                .await
                .map_err(convert_error)?,
        ))
    }

    /// 为某个用户新增并运行一个网络实例。
    async fn handle_run_user_network(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path((username, machine_id)): Path<(String, uuid::Uuid)>,
        Json(payload): Json<RunNetworkJsonReq>,
    ) -> Result<Json<Void>, HttpHandleError> {
        Self::require_admin(&auth_session).await?;

        let backend = auth_session.backend.clone();
        let target_id = Self::find_user_id(&backend, &username).await?;

        client_mgr
            .check_network_run_allowed(target_id, machine_id)
            .await
            .map_err(|e| (StatusCode::FORBIDDEN, other_error(e.to_string()).into()))?;
        client_mgr
            .handle_run_network_instance((target_id, machine_id), payload.config, payload.save)
            .await
            .map_err(convert_error)?;

        Ok(Void::default().into())
    }

    /// 修改某个用户的网络实例配置。
    async fn handle_save_user_network_config(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path((username, machine_id, inst_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
        Json(payload): Json<super::network::SaveNetworkJsonReq>,
    ) -> Result<Json<Void>, HttpHandleError> {
        Self::require_admin(&auth_session).await?;

        if payload.config.instance_id() != inst_id.to_string() {
            return Err((
                StatusCode::BAD_REQUEST,
                other_error("Instance ID mismatch".to_string()).into(),
            ));
        }

        let backend = auth_session.backend.clone();
        let target_id = Self::find_user_id(&backend, &username).await?;

        client_mgr
            .handle_save_network_config((target_id, machine_id), inst_id, payload.config)
            .await
            .map_err(convert_error)?;

        Ok(Void::default().into())
    }

    /// 删除某个用户的网络实例。
    async fn handle_remove_user_network(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path((username, machine_id, inst_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    ) -> Result<Json<Void>, HttpHandleError> {
        Self::require_admin(&auth_session).await?;

        let backend = auth_session.backend.clone();
        let target_id = Self::find_user_id(&backend, &username).await?;

        client_mgr
            .handle_remove_network_instances((target_id, machine_id), vec![inst_id])
            .await
            .map_err(convert_error)?;

        Ok(Void::default().into())
    }

    /// 启用 / 停用某个用户的网络实例。
    async fn handle_update_user_network_state(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path((username, machine_id, inst_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
        Json(payload): Json<UpdateNetworkStateJsonReq>,
    ) -> Result<Json<Void>, HttpHandleError> {
        Self::require_admin(&auth_session).await?;

        let backend = auth_session.backend.clone();
        let target_id = Self::find_user_id(&backend, &username).await?;

        if !payload.disabled {
            client_mgr
                .check_network_run_allowed(target_id, machine_id)
                .await
                .map_err(|e| (StatusCode::FORBIDDEN, other_error(e.to_string()).into()))?;
        }
        client_mgr
            .handle_update_network_state((target_id, machine_id), inst_id, payload.disabled)
            .await
            .map_err(convert_error)?;

        Ok(Void::default().into())
    }

    /// 读取某个用户的网络实例配置。
    async fn handle_get_user_network_config(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path((username, machine_id, inst_id)): Path<(String, uuid::Uuid, uuid::Uuid)>,
    ) -> Result<Json<NetworkConfig>, HttpHandleError> {
        Self::require_admin(&auth_session).await?;

        let backend = auth_session.backend.clone();
        let target_id = Self::find_user_id(&backend, &username).await?;

        Ok(Json(
            client_mgr
                .handle_get_network_config((target_id, machine_id), inst_id)
                .await
                .map_err(convert_error)?,
        ))
    }

    pub fn build_route() -> Router<AppStateInner> {
        Router::new()
            .route("/api/v1/admin/users", get(Self::handle_list_users))
            .route(
                "/api/v1/admin/users/:username",
                delete(Self::handle_delete_user),
            )
            .route(
                "/api/v1/admin/users/:username/role",
                post(Self::handle_set_role),
            )
            .route(
                "/api/v1/admin/users/:username/enabled",
                post(Self::handle_set_enabled),
            )
            .route(
                "/api/v1/admin/users/:username/expire",
                post(Self::handle_set_expire),
            )
            .route(
                "/api/v1/admin/users/:username/client-limit",
                post(Self::handle_set_client_limit),
            )
            .route(
                "/api/v1/admin/users/:username/password",
                post(Self::handle_reset_password),
            )
            .route(
                "/api/v1/admin/users/:username/machines",
                get(Self::handle_list_user_machines),
            )
            .route(
                "/api/v1/admin/users/:username/machines/:machine-id/kick",
                post(Self::handle_kick_machine),
            )
            .route(
                "/api/v1/admin/users/:username/machines/:machine-id/networks",
                get(Self::handle_list_user_networks).post(Self::handle_run_user_network),
            )
            .route(
                "/api/v1/admin/users/:username/machines/:machine-id/networks/:inst-id",
                delete(Self::handle_remove_user_network)
                    .put(Self::handle_update_user_network_state),
            )
            .route(
                "/api/v1/admin/users/:username/machines/:machine-id/networks/config/:inst-id",
                get(Self::handle_get_user_network_config)
                    .put(Self::handle_save_user_network_config),
            )
    }
}
