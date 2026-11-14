import { NetworkTypes } from 'sdwan-frontend-lib'

type NetworkConfig = NetworkTypes.NetworkConfig

/**
 * SDWAN GUI 预置参数
 * 这些值是 SDWAN 发行版的出厂默认值，新建网络时自动带入。
 */

/** 预置配置服务器地址 */
export const SDWAN_CONFIG_SERVER_URL = 'udp://oa.xiaolin.cc:22020/xlxxadmin'

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
 * 界面不提供修改入口，编辑网络时也会强制写回，
 * 避免已有配置或导入配置覆盖发行版默认值。
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
