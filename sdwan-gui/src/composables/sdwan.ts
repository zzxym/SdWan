/**
 * SDWAN GUI 预置参数
 *
 * 实现已迁移到共享库 `sdwan-frontend-lib`（与 Web 控制台共用），
 * 这里保留 re-export 以兼容现有引用。
 */
export {
    SDWAN_CONFIG_SERVER_ADDR,
    SDWAN_CONFIG_SERVER_USER,
    SDWAN_CONFIG_SERVER_URL,
    resolveConfigServerUrl,
    SDWAN_NETWORK_NAME,
    SDWAN_NETWORK_SECRET,
    SDWAN_PEER_URLS,
    SDWAN_LISTENER_URLS,
    sdwanPresetAdvancedSettings,
    createDefaultSdwanNetworkConfig,
} from 'sdwan-frontend-lib';
