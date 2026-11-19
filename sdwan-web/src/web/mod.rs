use axum::{
    Router,
    extract::State,
    http::header,
    response::{IntoResponse, Response},
    routing,
};
use axum_embed::ServeEmbed;
use rust_embed::RustEmbed;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_util::task::AbortOnDropHandle;

/// Embed assets for web dashboard, build frontend first
#[derive(RustEmbed, Clone)]
#[folder = "frontend/dist/"]
struct Assets;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct ApiMetaResponse {
    api_host: String,
}

async fn handle_api_meta(State(api_host): State<url::Url>) -> impl IntoResponse {
    Response::builder()
        .header(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )
        .header(header::CACHE_CONTROL, "no-cache, no-store, must-revalidate")
        .header(header::PRAGMA, "no-cache")
        .header(header::EXPIRES, "0")
        .body(format!(
            "window.apiMeta = {}",
            serde_json::to_string(&ApiMetaResponse {
                api_host: api_host.to_string()
            })
            .unwrap(),
        ))
        .unwrap()
}

pub fn build_router(api_host: Option<url::Url>) -> Router {
    let service = ServeEmbed::<Assets>::new();
    let router = Router::new();

    // 运营管理入口：`http://<host>:<port>/xlxxadmin`
    //
    // 前端使用 hash 路由，直接访问该路径拿不到 index.html（ServeEmbed 未命中即 404），
    // 因此这里返回一份注入了跳转脚本的 index.html，把 hash 路由指向管理页。
    let router = router.route("/xlxxadmin", routing::get(handle_admin_entry));

    let router = if let Some(api_host) = api_host {
        let sub_router = Router::new()
            .route("/api_meta.js", routing::get(handle_api_meta))
            .with_state(api_host);
        router.merge(sub_router)
    } else {
        router
    };

    router.fallback_service(service)
}

/// 运营管理入口页面：在 index.html 中注入脚本，把 hash 路由切到 `#/xlxxadmin`。
async fn handle_admin_entry() -> Response {
    let Some(index) = Assets::get("index.html") else {
        return Response::builder()
            .status(axum::http::StatusCode::NOT_FOUND)
            .body(axum::body::Body::from("index.html not found"))
            .unwrap();
    };

    let mut html = String::from_utf8_lossy(index.data.as_ref()).to_string();

    // 在应用脚本执行前跳到「站点根路径 + hash 路由」的标准形式 /#/xlxxadmin。
    // 不能停留在 /xlxxadmin 路径上再用相对 hash：前端 createWebHashHistory() 会把
    // location.pathname 当作 base，刷新时反复把 #/xlxxadmin 拼接到 hash 上。
    // 跳到根路径后，后续刷新由 fallback_service 提供不含本脚本的 index.html，
    // 从根上杜绝 xlxxadmin#/xlxxadmin#/... 的累加。
    const BOOTSTRAP: &str = r##"<script>if(window.location.pathname!=="/"||window.location.hash!=="#/xlxxadmin"){window.location.replace("/#/xlxxadmin"+window.location.search);}</script>"##;
    if let Some(pos) = html.find("</head>") {
        html.insert_str(pos, BOOTSTRAP);
    } else {
        html.insert_str(0, BOOTSTRAP);
    }

    Response::builder()
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache, no-store, must-revalidate")
        .body(axum::body::Body::from(html))
        .unwrap()
}

pub struct WebServer {
    bind_addr: SocketAddr,
    router: Router,
    serve_task: Option<AbortOnDropHandle<()>>,
}

impl WebServer {
    pub async fn new(bind_addr: SocketAddr, router: Router) -> anyhow::Result<Self> {
        Ok(WebServer {
            bind_addr,
            router,
            serve_task: None,
        })
    }

    pub async fn start(self) -> Result<AbortOnDropHandle<()>, anyhow::Error> {
        let listener = TcpListener::bind(self.bind_addr).await?;
        let app = self.router;

        let task = AbortOnDropHandle::new(tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        }));

        Ok(task)
    }
}
