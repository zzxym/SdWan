import { type } from '@tauri-apps/plugin-os'
import type { NetworkTypes } from 'sdwan-frontend-lib'

/**
 * 浏览器预览模式
 *
 * sdwan-gui 是 Tauri 应用，正常运行时前端通过 `invoke` 与 Rust 后端通信。
 * 在浏览器中直接打开 dev server 时没有 Tauri runtime，所有 invoke 都会失败，
 * 这里提供一份演示数据，仅用于本地预览界面布局，不会影响真实客户端行为。
 */
export function isPreviewMode(): boolean {
  return typeof window !== 'undefined' && !('__TAURI_INTERNALS__' in window)
}

/**
 * 安全的平台类型查询：没有 Tauri runtime 时返回 'unknown'，
 * 避免插件抛错中断组件渲染。
 */
export function safeOsType(): string {
  try {
    return type()
  }
  catch {
    return 'unknown'
  }
}

export const PREVIEW_INSTANCE_ID = '6f1b7d3e-2a44-4f7c-9b0e-3d5a8c2f1e90'
export const PREVIEW_DISABLED_INSTANCE_ID = 'b2c4e8a1-7f36-4d59-8a2b-6c9e0f3d5a71'

export interface PreviewInstance {
  uuid: string
  name: string
  running: boolean
}

export const PREVIEW_INSTANCES: PreviewInstance[] = [
  { uuid: PREVIEW_INSTANCE_ID, name: 'xlxxadmin', running: true },
  { uuid: PREVIEW_DISABLED_INSTANCE_ID, name: 'demo-network', running: false },
]

function stunInfo(): NetworkTypes.StunInfo {
  return {
    udp_nat_type: 1,
    tcp_nat_type: 1,
    last_update_time: Date.now(),
  }
}

function peerRoutePair(peerId: number, hostname: string, ipv4: number, latencyUs: number, cost: number): NetworkTypes.PeerRoutePair {
  return {
    route: {
      peer_id: peerId,
      ipv4_addr: { address: { addr: ipv4 }, network_length: 24 },
      next_hop_peer_id: peerId,
      cost,
      proxy_cidrs: [],
      hostname,
      stun_info: stunInfo(),
      inst_id: PREVIEW_INSTANCE_ID,
      version: '2.6.4',
      feature_flag: {
        is_public_server: false,
        avoid_relay_data: false,
      },
    } as NetworkTypes.Route,
    peer: {
      peer_id: peerId,
      conns: [{
        conn_id: `conn-${peerId}`,
        my_peer_id: 123456789,
        is_client: false,
        peer_id: peerId,
        features: [],
        tunnel: {
          tunnel_type: 'tcp',
          local_addr: { url: 'tcp://10.0.0.2:11010' },
          remote_addr: { url: 'tcp://203.0.113.10:11010' },
        },
        stats: {
          rx_bytes: 10485760,
          tx_bytes: 5242880,
          rx_packets: 12000,
          tx_packets: 6000,
          latency_us: latencyUs,
        },
        loss_rate: 0,
      }],
    },
  }
}

export function previewNetworkInfo(): NetworkTypes.NetworkInstanceRunningInfo {
  return {
    dev_name: 'sdwan0',
    my_node_info: {
      virtual_ipv4: { address: { addr: 0x0A000002 }, network_length: 24 },
      hostname: 'sdwan-client-demo',
      version: '2.6.4',
      ips: {
        public_ipv4: { addr: 0xCB00710A },
        interface_ipv4s: [{ addr: 0x0A000002 }],
        public_ipv6: { part1: 0, part2: 0, part3: 0, part4: 1 },
        interface_ipv6s: [],
        listeners: [],
      },
      stun_info: stunInfo(),
      listeners: [
        { url: 'tcp://0.0.0.0:10010' },
        { url: 'udp://0.0.0.0:10010' },
      ],
      vpn_portal_cfg: '',
      peer_id: 123456789,
    },
    events: [],
    routes: [],
    peers: [],
    peer_route_pairs: [
      peerRoutePair(111111111, 'jpnode1.xldns.top', 0x0A000003, 32000, 1),
      peerRoutePair(222222222, 'sghko.xldns.top', 0x0A000004, 86000, 2),
    ],
    running: true,
  }
}
