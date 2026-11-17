use axum::{
    Router,
    http::StatusCode,
    routing::{get, post, put},
};
use axum_login::login_required;
use axum_messages::Message;
use serde::{Deserialize, Serialize};

use crate::restful::users::{Backend, UserUsable};

use std::sync::Arc;

use crate::FeatureFlags;

use super::{
    AppStateInner,
    users::{AuthSession, Credentials},
};

#[derive(Debug, Deserialize, Serialize)]
pub struct LoginResult {
    messages: Vec<Message>,
}

/// 验证码信息（JSON 形式），前端可直接用 `image` 作为 `<img>` 的 src，
/// 并在注册时把 `captcha_id` 一起提交。
#[derive(Debug, Deserialize, Serialize)]
pub struct CaptchaInfo {
    /// 验证码标识，注册时需随请求提交
    pub captcha_id: String,
    /// Base64 编码的图片（`data:image/png;base64,...`）
    pub image: String,
    /// 有效期（秒）
    pub expires_in: u64,
}

pub fn router(captcha_store: Arc<super::captcha::CaptchaStore>) -> Router<AppStateInner> {
    let r = Router::new()
        .route("/api/v1/auth/password", put(self::put::change_password))
        .route(
            "/api/v1/auth/check_login_status",
            get(self::get::check_login_status),
        )
        .route_layer(login_required!(Backend));
    Router::new()
        .merge(r)
        .route("/api/v1/auth/login", post(self::post::login))
        .route("/api/v1/auth/logout", get(self::get::logout))
        .route("/api/v1/auth/captcha", get(self::get::get_captcha))
        .route(
            "/api/v1/auth/captcha_info",
            get(self::get::get_captcha_info),
        )
        .route("/api/v1/auth/register", post(self::post::register))
        .layer(axum::Extension(captcha_store))
}

mod put {
    use axum::Json;
    use axum_login::AuthUser;
    use sdwan::proto::common::Void;

    use crate::restful::{HttpHandleError, other_error, users::ChangePassword};

    use super::*;

    pub async fn change_password(
        mut auth_session: AuthSession,
        Json(req): Json<ChangePassword>,
    ) -> Result<Json<Void>, HttpHandleError> {
        if let Err(e) = auth_session
            .backend
            .change_password(auth_session.user.as_ref().unwrap().id(), &req)
            .await
        {
            tracing::error!("Failed to change password: {:?}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(other_error(format!("{:?}", e))),
            ));
        }

        let _ = auth_session.logout().await;

        Ok(Void::default().into())
    }
}

mod post {
    use axum::{Json, extract::Extension};
    use sdwan::proto::common::Void;

    use crate::restful::{
        HttpHandleError,
        captcha::{
            CaptchaStore,
            extension::{CaptchaUtil, axum_tower_sessions::CaptchaAxumTowerSessionStaticExt},
        },
        other_error,
        users::RegisterNewUser,
    };

    use super::*;

    pub async fn login(
        mut auth_session: AuthSession,
        Json(creds): Json<Credentials>,
    ) -> Result<Json<Void>, HttpHandleError> {
        let user = match auth_session.authenticate(creds.clone()).await {
            Ok(Some(user)) => user,
            Ok(None) => {
                // 口令正确但账号被禁用/过期时给出明确原因，便于客户端提示用户。
                if let Some(reason) = account_state_reason(&auth_session, &creds.username).await {
                    return Err((StatusCode::FORBIDDEN, Json::from(other_error(reason))));
                }
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json::from(other_error("Invalid credentials")),
                ));
            }
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json::from(other_error(format!("{:?}", e))),
                ));
            }
        };

        if let Err(e) = auth_session.login(&user).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json::from(other_error(format!("{:?}", e))),
            ));
        }

        Ok(Void::default().into())
    }

    /// 查询账号状态，仅在口令正确但账号不可用时返回原因描述。
    async fn account_state_reason(
        auth_session: &AuthSession,
        username: &str,
    ) -> Option<String> {
        use crate::db::entity::users;
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        let user = users::Entity::find()
            .filter(users::Column::Username.eq(username.to_string()))
            .one(auth_session.backend.db().orm_db())
            .await
            .ok()
            .flatten()?;

        if !user.enabled {
            return Some("Account is disabled by administrator".to_string());
        }
        if user.is_expired() {
            return Some("Account has expired".to_string());
        }
        None
    }

    pub async fn register(
        Extension(feature_flags): Extension<Arc<FeatureFlags>>,
        Extension(captcha_store): Extension<Arc<CaptchaStore>>,
        auth_session: AuthSession,
        captcha_session: tower_sessions::Session,
        Json(req): Json<RegisterNewUser>,
    ) -> Result<Json<Void>, HttpHandleError> {
        // Check if registration is disabled
        if feature_flags.disable_registration {
            tracing::warn!("Registration attempt blocked: registration is disabled");
            return Err((
                StatusCode::FORBIDDEN,
                other_error("Registration is disabled").into(),
            ));
        }

        // 校验验证码：
        // 优先使用 captcha_id（与 session cookie 无关，跨站嵌入等场景同样可用）；
        // 兼容旧客户端：未携带 captcha_id 时回退到 session 校验。
        let captcha_ok = match req.captcha_id.as_deref().filter(|id| !id.is_empty()) {
            Some(captcha_id) => captcha_store.verify(captcha_id, &req.captcha),
            None => CaptchaUtil::ver(&req.captcha, &captcha_session).await,
        };
        if !captcha_ok {
            return Err((
                StatusCode::BAD_REQUEST,
                other_error(format!("captcha verify error, input: {}", req.captcha)).into(),
            ));
        }

        if let Err(e) = auth_session.backend.register_new_user(&req).await {
            tracing::error!("Failed to register new user: {:?}", e);
            return Err((
                StatusCode::BAD_REQUEST,
                other_error(format!("{:?}", e)).into(),
            ));
        }

        Ok(Void::default().into())
    }
}

mod get {
    use crate::restful::{
        HttpHandleError,
        captcha::{
            CaptchaStore, NewCaptcha as _,
            builder::spec::SpecCaptcha,
            extension::{CaptchaUtil, axum_tower_sessions::CaptchaAxumTowerSessionExt as _},
        },
        other_error,
    };
    use axum::{Json, extract::Extension, response::Response};
    use sdwan::proto::common::Void;
    use tower_sessions::Session;

    use super::*;

    /// 验证码有效期（秒）
    const CAPTCHA_EXPIRES_IN: u64 = 300;

    pub async fn logout(mut auth_session: AuthSession) -> Result<Json<Void>, HttpHandleError> {
        match auth_session.logout().await {
            Ok(_) => Ok(Json(Void::default())),
            Err(e) => {
                tracing::error!("Failed to logout: {:?}", e);
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json::from(other_error(format!("{:?}", e))),
                ))
            }
        }
    }

    /// 返回 PNG 图片形式的验证码（兼容旧客户端）。
    ///
    /// 同时会登记一个 `captcha_id`，可通过响应头 `X-Captcha-Id` 获取。
    pub async fn get_captcha(
        Extension(captcha_store): Extension<Arc<CaptchaStore>>,
        session: Session,
    ) -> Result<Response, HttpHandleError> {
        let mut captcha: CaptchaUtil<SpecCaptcha> = CaptchaUtil::with_size_and_len(127, 48, 4);
        let mut response = match captcha.out(&session).await {
            Ok(response) => response,
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json::from(other_error(format!("{:?}", e))),
                ));
            }
        };

        let answer = captcha.answer();
        let captcha_id = captcha_store.issue(&answer);
        if let Ok(value) = axum::http::HeaderValue::from_str(&captcha_id) {
            response.headers_mut().insert("x-captcha-id", value);
        }

        Ok(response)
    }

    /// 返回 JSON 形式的验证码信息，携带 `captcha_id`，校验时不再依赖 session cookie。
    pub async fn get_captcha_info(
        Extension(captcha_store): Extension<Arc<CaptchaStore>>,
    ) -> Result<Json<CaptchaInfo>, HttpHandleError> {
        let mut captcha: CaptchaUtil<SpecCaptcha> = CaptchaUtil::with_size_and_len(127, 48, 4);
        let image = match captcha.base64() {
            Ok(image) => image,
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json::from(other_error(format!("{:?}", e))),
                ));
            }
        };

        let answer = captcha.answer();
        let captcha_id = captcha_store.issue(&answer);

        Ok(Json(CaptchaInfo {
            captcha_id,
            image,
            expires_in: CAPTCHA_EXPIRES_IN,
        }))
    }

    pub async fn check_login_status(
        auth_session: AuthSession,
    ) -> Result<Json<Void>, HttpHandleError> {
        if auth_session.user.is_some() {
            Ok(Json(Void::default()))
        } else {
            Err((
                StatusCode::UNAUTHORIZED,
                Json::from(other_error("Not logged in")),
            ))
        }
    }
}
