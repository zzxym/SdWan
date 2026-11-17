import { invoke } from '@tauri-apps/api/core'
import { Api, NetworkTypes } from 'sdwan-frontend-lib'
import { GetNetworkMetasResponse } from 'node_modules/sdwan-frontend-lib/dist/modules/api'


type NetworkConfig = NetworkTypes.NetworkConfig
type ValidateConfigResponse = Api.ValidateConfigResponse
type ListNetworkInstanceIdResponse = Api.ListNetworkInstanceIdResponse
type ConfigSource = 'user' | 'webhook' | 'legacy'
interface ServiceOptions {
  config_dir: string
  rpc_portal: string
  file_log_level: string
  file_log_dir: string
  config_server?: string;
}

export type ServiceStatus = "Running" | "Stopped" | "NotInstalled"

/**
 * SDWAN 精简客户端为只读模式。
 *
 * 网络配置只能由 Web 控制台（sdwan-web）通过 RPC 下发，GUI 侧不提供任何写入能力，
 * 因此这里的写入类入口统一拒绝，避免出现可被本地绕过的配置后门。
 */
export const READ_ONLY_ERROR = 'This SDWAN client is read-only: network configuration must be issued from the web console.'

function rejectReadOnly(): never {
  throw new Error(READ_ONLY_ERROR)
}

interface StoredGuiConfig {
  config: NetworkConfig
  source: ConfigSource
}

function parseStoredConfigs(raw: string | null): StoredGuiConfig[] {
  const parsed: unknown = JSON.parse(raw || '[]')
  if (!Array.isArray(parsed)) {
    return []
  }

  return parsed.flatMap((entry): StoredGuiConfig[] => {
    if (entry && typeof entry === 'object' && 'config' in entry) {
      const { config, source } = entry as {
        config?: NetworkConfig
        source?: ConfigSource
      }
      if (!config) {
        return []
      }
      return [{
        config: NetworkTypes.normalizeNetworkConfig(config),
        source: source === 'user' || source === 'webhook' ? source : 'legacy',
      }]
    }

    return [{
      config: NetworkTypes.normalizeNetworkConfig(entry as NetworkConfig),
      source: 'legacy',
    }]
  })
}

// ---------------------------------------------------------------------------
// 只读接口：状态查询与运行时管理
// ---------------------------------------------------------------------------

export async function collectNetworkInfo(instanceId: string) {
  return await invoke<Api.CollectNetworkInfoResponse>('collect_network_info', { instanceId })
}

export async function setLoggingLevel(level: string) {
  return await invoke('set_logging_level', { level })
}

export async function setTunFd(fd: number) {
  return await invoke('set_tun_fd', { fd })
}

export async function getSdwanVersion() {
  return await invoke<string>('sdwan_version')
}

export async function getLocalIpv4() {
  return await invoke<string>('local_ipv4')
}

/** 退出登录时停止并删除客户端上的全部网络实例及其配置。 */
export async function removeNetworkInstances(instanceIds: string[]) {
  return await invoke<void>('remove_network_instances', { instanceIds })
}

export async function listNetworkInstanceIds() {
  return await invoke<ListNetworkInstanceIdResponse>('list_network_instance_ids')
}

export async function getConfig(instanceId: string) {
  const config = await invoke<NetworkConfig>('get_config', { instanceId })
  return NetworkTypes.normalizeNetworkConfig(config)
}

export async function getNetworkMetas(instanceIds: string[]) {
  return await invoke<GetNetworkMetasResponse>('get_network_metas', { instanceIds })
}

/**
 * 恢复上次由 Web 控制台下发的网络配置（本地仅作镜像，不做本地编辑）。
 */
export async function sendConfigs(enabledNetworks: string[]) {
  const networkList = parseStoredConfigs(localStorage.getItem('networkList'))
  return await invoke('load_configs', {
    configs: networkList.map(({ config, source }) => ({
      config: NetworkTypes.toBackendNetworkConfig(config),
      source,
    })),
    enabledNetworks
  })
}

export async function initService(opts?: ServiceOptions) {
  return await invoke('init_service', { opts })
}

export async function setServiceStatus(enable: boolean) {
  return await invoke('set_service_status', { enable })
}

export async function getServiceStatus() {
  return await invoke<ServiceStatus>('get_service_status')
}

export async function initRpcConnection(isNormalMode: boolean, url?: string) {
  return await invoke('init_rpc_connection', { isNormalMode, url })
}

export async function isClientRunning() {
  return await invoke<boolean>('is_client_running')
}

export async function setAllowedNetworkName(username: string) {
  return await invoke<void>('set_allowed_network_name', { username })
}

export async function initWebClient(url?: string) {
  return await invoke('init_web_client', { url })
}

export async function isWebClientConnected() {
  return await invoke<boolean>('is_web_client_connected')
}

// ---------------------------------------------------------------------------
// 写入类接口：只读客户端一律拒绝
// ---------------------------------------------------------------------------

export async function parseNetworkConfig(_cfg: NetworkConfig): Promise<string> {
  return rejectReadOnly()
}

export async function generateNetworkConfig(_tomlConfig: string): Promise<NetworkConfig> {
  return rejectReadOnly()
}

export async function runNetworkInstance(_cfg: NetworkConfig, _save: boolean): Promise<void> {
  return rejectReadOnly()
}

export async function deleteNetworkInstance(_instanceId: string): Promise<void> {
  return rejectReadOnly()
}

export async function updateNetworkConfigState(_instanceId: string, _disabled: boolean): Promise<void> {
  return rejectReadOnly()
}

export async function saveNetworkConfig(_cfg: NetworkConfig): Promise<void> {
  return rejectReadOnly()
}

export async function validateConfig(_cfg: NetworkConfig): Promise<ValidateConfigResponse> {
  return rejectReadOnly()
}
