use std::sync::Arc;

use crate::{
    common::{
        MachineIdOptions,
        config::TomlConfigLoader,
        global_ctx::{ArcGlobalCtx, GlobalCtx},
        log,
        os_info::collect_device_os_info,
        resolve_machine_id,
        stun::MockStunInfoCollector,
    },
    connector::create_connector_by_url,
    instance_manager::{DaemonGuard, NetworkInstanceManager},
    proto::common::NatType,
    proto::web::HeartbeatResponse,
    tunnel::{IpVersion, Tunnel, TunnelConnector, TunnelError, TunnelScheme},
};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use tokio_util::task::AbortOnDropHandle;
use url::Url;
use uuid::Uuid;

#[async_trait]
pub trait WebClientHooks: Send + Sync {
    async fn pre_run_network_instance(&self, _cfg: &TomlConfigLoader) -> Result<(), String> {
        Ok(())
    }

    async fn post_run_network_instance(&self, _id: &Uuid) -> Result<(), String> {
        Ok(())
    }

    async fn post_remove_network_instances(&self, _ids: &[Uuid]) -> Result<(), String> {
        Ok(())
    }
}

pub struct DefaultHooks;

#[async_trait]
impl WebClientHooks for DefaultHooks {}

pub mod controller;
pub mod security;
pub mod session;

/// 预置配置服务器地址（主机与端口），用户只填写用户名时使用。
pub const DEFAULT_CONFIG_SERVER_ADDR: &str = "sdwanweb.xiaolin.cc:22020";

/// 预置配置服务器用户名，配置服务器地址为空时使用。
pub const DEFAULT_CONFIG_SERVER_USER: &str = "xlxxadmin";

/// 解析配置服务器地址。
///
/// - 地址为空时，使用预置的 `udp://sdwanweb.xiaolin.cc:22020/xlxxadmin`；
/// - 用户指定了完整地址（含 scheme，例如 `udp://sdwanweb.xiaolin.cc:22020/xlxxadmin`）时，按用户指定的地址连接；
/// - 只填写了用户名（例如 `xlxxadmin`）时，补全为 `udp://sdwanweb.xiaolin.cc:22020/xlxxadmin`。
fn resolve_config_server_url(config_server_url_s: &str) -> Result<Url> {
    let raw = config_server_url_s.trim();
    if raw.is_empty() {
        // 未填写地址时回退到预置配置服务器。
        return Url::parse(&format!(
            "udp://{DEFAULT_CONFIG_SERVER_ADDR}/{DEFAULT_CONFIG_SERVER_USER}"
        ))
        .with_context(|| "failed to build default config server URL");
    }

    // 带 scheme 说明用户指定了完整地址，直接使用。
    if raw.contains("://") {
        return Url::parse(raw)
            .with_context(|| format!("failed to parse config server URL: {raw}"));
    }

    // 只给了用户名，补全到预置配置服务器。
    let mut url = Url::parse(&format!("udp://{DEFAULT_CONFIG_SERVER_ADDR}/"))
        .with_context(|| "failed to build default config server URL")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("failed to build default config server URL"))?
        .extend(raw.split('/'));
    Ok(url)
}

use std::sync::atomic::{AtomicBool, Ordering};

pub struct WebClient {
    controller: Arc<controller::Controller>,
    tasks: AbortOnDropHandle<()>,
    manager_guard: DaemonGuard,
    connected: Arc<AtomicBool>,
    heartbeat_resp: Arc<tokio::sync::Mutex<Option<HeartbeatResponse>>>,
}

struct ConfigServerConnector {
    url: Url,
    global_ctx: ArcGlobalCtx,
}

#[async_trait]
impl TunnelConnector for ConfigServerConnector {
    async fn connect(&mut self) -> std::result::Result<Box<dyn Tunnel>, TunnelError> {
        let mut connector =
            create_connector_by_url(self.url.as_str(), &self.global_ctx, IpVersion::Both)
                .await
                .map_err(|err| match err {
                    crate::common::error::Error::TunnelError(err) => err,
                    err => TunnelError::Anyhow(err.into()),
                })?;

        connector.connect().await
    }

    fn remote_url(&self) -> Url {
        self.url.clone()
    }
}

impl WebClient {
    pub fn new<T: TunnelConnector + 'static, S: ToString, H: ToString>(
        connector: T,
        token: S,
        machine_id: Uuid,
        hostname: H,
        secure_mode: bool,
        manager: Arc<NetworkInstanceManager>,
        hooks: Option<Arc<dyn WebClientHooks>>,
    ) -> Self {
        let manager_guard = manager.register_daemon();
        let hooks = hooks.unwrap_or_else(|| Arc::new(DefaultHooks));
        let controller = Arc::new(controller::Controller::new(
            token.to_string(),
            machine_id,
            hostname.to_string(),
            collect_device_os_info(),
            manager,
            hooks,
        ));
        let connected = Arc::new(AtomicBool::new(false));
        let heartbeat_resp = Arc::new(tokio::sync::Mutex::new(None));

        let controller_clone = controller.clone();
        let connected_clone = connected.clone();
        let heartbeat_resp_clone = heartbeat_resp.clone();
        let tasks = AbortOnDropHandle::new(tokio::spawn(async move {
            Self::routine(
                controller_clone,
                connected_clone,
                secure_mode,
                heartbeat_resp_clone,
                Box::new(connector),
            )
            .await;
        }));

        WebClient {
            controller,
            tasks,
            manager_guard,
            connected,
            heartbeat_resp,
        }
    }

    async fn routine(
        controller: Arc<controller::Controller>,
        connected: Arc<AtomicBool>,
        secure_mode: bool,
        heartbeat_resp: Arc<tokio::sync::Mutex<Option<HeartbeatResponse>>>,
        mut connector: Box<dyn TunnelConnector>,
    ) {
        loop {
            let conn = match connector.connect().await {
                Ok(conn) => conn,
                Err(error) => {
                    let wait = 1;
                    log::warn!(%error, "Failed to connect to the server, retrying in {} seconds...", wait);
                    tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                    continue;
                }
            };

            connected.store(true, Ordering::Release);
            log::info!("Successfully connected to {:?}", conn.info());

            let mut session =
                session::Session::new(conn, controller.clone(), heartbeat_resp.clone());
            let support_encryption = match tokio::time::timeout(
                std::time::Duration::from_secs(3),
                session.get_feature(),
            )
            .await
            {
                Ok(Ok(feature)) => feature.support_encryption,
                Ok(Err(error)) => {
                    log::warn!(%error, "GetFeature rpc failed, fallback to legacy tunnel");
                    false
                }
                Err(_) => {
                    log::warn!("GetFeature rpc timeout, fallback to legacy tunnel");
                    false
                }
            };

            if support_encryption && security::web_secure_tunnel_supported() {
                log::info!("Server supports encryption, reconnecting with secure tunnel");
                drop(session);

                let mut secure_tunnel = None;
                for prologue in security::NOISE_PROLOGUES {
                    let tunnel = match connector.connect().await {
                        Ok(conn) => conn,
                        Err(error) => {
                            connected.store(false, Ordering::Release);
                            let wait = 1;
                            log::warn!(%error, "Failed to reconnect secure tunnel, retrying in {} seconds...", wait);
                            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                            break;
                        }
                    };

                    match security::upgrade_client_tunnel(tunnel, prologue).await {
                        Ok(conn) => {
                            secure_tunnel = Some(conn);
                            break;
                        }
                        Err(error) => {
                            // 尝试下一个 prologue，兼容尚未升级到 sdwan-core 的服务端。
                            log::warn!(%error, prologue = ?prologue, "Noise handshake failed, trying the next prologue");
                        }
                    }
                }

                let conn = match secure_tunnel {
                    Some(conn) => conn,
                    None => {
                        // 所有握手都失败时降级到普通隧道继续工作。
                        // 服务端可能与本地版本不兼容，若在此无限重试将永远无法建立会话，
                        // 设备也就无法在 Web 控制台的设备列表中出现。
                        log::warn!("All Noise handshakes failed, falling back to legacy tunnel");
                        match connector.connect().await {
                            Ok(conn) => conn,
                            Err(error) => {
                                connected.store(false, Ordering::Release);
                                let wait = 1;
                                log::warn!(%error, "Failed to reconnect legacy tunnel, retrying in {} seconds...", wait);
                                tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                                continue;
                            }
                        }
                    }
                };

                let mut session =
                    session::Session::new(conn, controller.clone(), heartbeat_resp.clone());
                session.start_heartbeat().await;
                session.wait().await;
                connected.store(false, Ordering::Release);
                continue;
            }

            if support_encryption {
                if secure_mode {
                    connected.store(false, Ordering::Release);
                    let wait = 1;
                    log::warn!(
                        "secure-mode enabled but local build lacks aes-gcm support for web secure tunnel, retrying in {} seconds...",
                        wait
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                    continue;
                }

                log::warn!(
                    "Server supports encryption but local build lacks aes-gcm support for web secure tunnel, falling back to legacy tunnel"
                );
            }

            if secure_mode {
                connected.store(false, Ordering::Release);
                let wait = 1;
                log::warn!(
                    "secure-mode enabled but server does not support encryption, retrying in {} seconds...",
                    wait
                );
                tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                continue;
            }

            session.start_heartbeat().await;
            session.wait().await;
            connected.store(false, Ordering::Release);
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    /// 返回最近一次心跳响应中的用户授权与使用期限信息（在线数/总授权数、到期时间）。
    pub async fn client_limit_info(&self) -> Option<HeartbeatResponse> {
        self.heartbeat_resp.lock().await.clone()
    }
}

pub async fn run_web_client(
    config_server_url_s: &str,
    machine_id_opts: MachineIdOptions,
    hostname: Option<String>,
    secure_mode: bool,
    manager: Arc<NetworkInstanceManager>,
    hooks: Option<Arc<dyn WebClientHooks>>,
) -> Result<WebClient> {
    let machine_id = resolve_machine_id(&machine_id_opts)
        .with_context(|| "failed to resolve machine id for web client")?;
    let config_server_url = resolve_config_server_url(config_server_url_s)?;

    TunnelScheme::try_from(&config_server_url).map_err(|_| {
        anyhow::anyhow!(
            "unsupported config server scheme: {}",
            config_server_url.scheme()
        )
    })?;

    let mut c_url = config_server_url.clone();
    if !matches!(c_url.scheme(), "ws" | "wss") {
        c_url.set_path("");
    }
    let token = config_server_url
        .path_segments()
        .and_then(|mut x| x.next_back())
        .map(|x| percent_encoding::percent_decode_str(x).decode_utf8())
        .transpose()
        .with_context(|| "failed to decode config server token")?
        .map(|x| x.to_string())
        .unwrap_or_default();

    if token.is_empty() {
        return Err(anyhow::anyhow!("empty token"));
    }

    let config = TomlConfigLoader::default();
    let global_ctx = Arc::new(GlobalCtx::new(config));
    global_ctx.replace_stun_info_collector(Box::new(MockStunInfoCollector {
        udp_nat_type: NatType::Unknown,
    }));
    let mut flags = global_ctx.get_flags();
    flags.bind_device = false;
    global_ctx.set_flags(flags);

    let hostname = match hostname {
        None => gethostname::gethostname().to_string_lossy().to_string(),
        Some(hostname) => hostname,
    };
    Ok(WebClient::new(
        ConfigServerConnector {
            url: c_url,
            global_ctx,
        },
        token.to_string(),
        machine_id,
        hostname,
        secure_mode,
        manager,
        hooks,
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::AtomicBool};

    use crate::{common::MachineIdOptions, instance_manager::NetworkInstanceManager};

    use super::*;

    #[test]
    fn resolve_config_server_url_keeps_user_specified_address() {
        let url = resolve_config_server_url("udp://sdwanweb.xiaolin.cc:22020/xlxxadmin").unwrap();
        assert_eq!(url.scheme(), "udp");
        assert_eq!(url.host_str(), Some("sdwanweb.xiaolin.cc"));
        assert_eq!(url.port(), Some(22020));
        assert_eq!(url.path(), "/xlxxadmin");
    }

    #[test]
    fn resolve_config_server_url_expands_bare_username() {
        let url = resolve_config_server_url("xlxxadmin").unwrap();
        assert_eq!(url.as_str(), "udp://sdwanweb.xiaolin.cc:22020/xlxxadmin");
    }

    #[test]
    fn resolve_config_server_url_expands_username_with_spaces() {
        let url = resolve_config_server_url("  my user  ").unwrap();
        assert_eq!(url.as_str(), "udp://sdwanweb.xiaolin.cc:22020/my%20user");
    }

    #[test]
    fn resolve_config_server_url_falls_back_to_preset_when_empty() {
        let url = resolve_config_server_url("").unwrap();
        assert_eq!(url.as_str(), "udp://sdwanweb.xiaolin.cc:22020/xlxxadmin");
        let url = resolve_config_server_url("   ").unwrap();
        assert_eq!(url.as_str(), "udp://sdwanweb.xiaolin.cc:22020/xlxxadmin");
    }

    #[test]
    fn resolve_config_server_url_rejects_malformed_address() {
        assert!(resolve_config_server_url("udp://").is_err());
    }

    #[tokio::test]
    async fn test_manager_wait() {
        let manager = Arc::new(NetworkInstanceManager::new());
        let temp_dir = tempfile::tempdir().unwrap();
        let client = super::run_web_client(
            format!("ring://{}/test", uuid::Uuid::new_v4()).as_str(),
            MachineIdOptions {
                explicit_machine_id: None,
                state_dir: Some(temp_dir.path().to_path_buf()),
            },
            None,
            false,
            manager.clone(),
            None,
        )
        .await
        .unwrap();
        let sleep_finish = Arc::new(AtomicBool::new(false));
        let sleep_finish_clone = sleep_finish.clone();

        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            println!("Dropping client...");
            sleep_finish_clone.store(true, std::sync::atomic::Ordering::Relaxed);
            drop(client);
            println!("Client dropped.");
        });

        println!("Waiting for manager...");
        manager.wait().await;
        assert!(sleep_finish.load(std::sync::atomic::Ordering::Relaxed));
        println!("Manager stopped.");
    }

    #[tokio::test]
    async fn test_run_web_client_with_unreachable_config_server() {
        let manager = Arc::new(NetworkInstanceManager::new());
        let temp_dir = tempfile::tempdir().unwrap();
        let client = super::run_web_client(
            "udp://config-server.invalid:22020/test",
            MachineIdOptions {
                explicit_machine_id: None,
                state_dir: Some(temp_dir.path().to_path_buf()),
            },
            None,
            false,
            manager,
            None,
        )
        .await
        .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!client.is_connected());
        drop(client);
    }
}
