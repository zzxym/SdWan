import * as NetworkTypes from '../types/network'

type NetworkConfig = NetworkTypes.NetworkConfig

/**
 * SDWAN 发行版预置参数
 *
 * GUI 与 Web 控制台共用：新建网络配置时自动带入预置的服务器节点与默认信息。
 */

/** 预置配置服务器地址（主机与端口），用户只填写用户名时使用 */
export const SDWAN_CONFIG_SERVER_ADDR = 'sdwanweb.xiaolin.cc:22020'

/** 预置配置服务器用户名 */
export const SDWAN_CONFIG_SERVER_USER = 'xlxxadmin'

/** 预置配置服务器地址 */
export const SDWAN_CONFIG_SERVER_URL = `udp://${SDWAN_CONFIG_SERVER_ADDR}/${SDWAN_CONFIG_SERVER_USER}`

/**
 * 归一化配置服务器地址：
 * - 用户指定了完整地址（含 scheme，例如 udp://sdwanweb.xiaolin.cc:22020/xlxxadmin）时，按用户指定的地址连接；
 * - 只填写了用户名（例如 xlxxadmin）时，补全为 udp://sdwanweb.xiaolin.cc:22020/xlxxadmin。
 */
export function resolveConfigServerUrl(input?: string): string {
    const raw = (input ?? '').trim()
    if (!raw) {
        return SDWAN_CONFIG_SERVER_URL
    }
    if (raw.includes('://')) {
        return raw
    }
    return `udp://${SDWAN_CONFIG_SERVER_ADDR}/${raw}`
}

/** 预置网络名称 */
export const SDWAN_NETWORK_NAME = 'xlxxadmin'

/** 预置网络密码 */
export const SDWAN_NETWORK_SECRET = 'Aa123456'

/** 预置初始节点（中继/接入服务器） */
export const SDWAN_PEER_URLS: string[] = [
    'tcp://oa.xiaolin.cc:10010',
    'udp://oa.xiaolin.cc:10010',
    'tcp://sdwan.xiaolin.cc:10010',
    'udp://sdwan.xiaolin.cc:10010',
    'tcp://jpnode1.xldns.top:16684',
    'udp://jpnode1.xldns.top:16684',
    'tcp://sghko.xldns.top:16684',
    'udp://sghko.xldns.top:16684',
]

/** 预置监听地址 */
export const SDWAN_LISTENER_URLS: string[] = [
    'tcp://0.0.0.0:10010',
    'udp://0.0.0.0:10010',
]

/**
 * SDWAN 出厂预置的高级设置
 *
 * GUI 界面不提供修改入口，编辑网络时也会强制写回；
 * Web 控制台仅在新建配置时带入默认值，仍允许用户自行调整。
 */
export function sdwanPresetAdvancedSettings(): Partial<NetworkConfig> {
    return {
        latency_first: true,
        ipv6_public_addr_auto: true,
        enable_kcp_proxy: true,
        bind_device: true,
        multi_thread: true,
        use_smoltcp: true,
        enable_exit_node: true,
        // '*' 通配符表示允许转发所有网络；留空反而会禁用转发
        enable_relay_network_whitelist: true,
        relay_network_whitelist: ['*'],
    }
}

/**
 * 生成 SDWAN 出厂默认网络配置
 */
export function createDefaultSdwanNetworkConfig(): NetworkConfig {
    const cfg = NetworkTypes.DEFAULT_NETWORK_CONFIG()

    cfg.network_name = SDWAN_NETWORK_NAME
    cfg.network_secret = SDWAN_NETWORK_SECRET
    cfg.peer_urls = [...SDWAN_PEER_URLS]

    cfg.listener_urls = [...SDWAN_LISTENER_URLS]

    Object.assign(cfg, sdwanPresetAdvancedSettings())

    return cfg
}
