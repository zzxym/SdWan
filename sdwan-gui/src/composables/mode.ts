import { resolveConfigServerUrl } from './sdwan';
import { safeOsType } from './preview';

export interface WebClientConfig {
    config_server_url?: string
}

export interface NormalMode extends WebClientConfig {
    mode: 'normal'
    // if not provided will use ring tunnel rpc server
    rpc_portal?: string
    enable_rpc_port_listen?: boolean
    rpc_listen_port?: number
}

export interface ServiceMode extends WebClientConfig {
    mode: 'service'
    config_dir: string
    rpc_portal: string
    file_log_level: 'off' | 'warn' | 'info' | 'debug' | 'trace'
    file_log_dir: string
    installed_core_version?: string
}

export interface RemoteMode {
    mode: 'remote'
    remote_rpc_address: string
}

export function saveMode(mode: Mode) {
    localStorage.setItem('app_mode', JSON.stringify(mode))
}


// SDWAN: 未设置配置服务器时使用预置地址；只填用户名时补全为预置配置服务器地址
function withDefaultConfigServer(mode: Mode): Mode {
    if (mode.mode === 'remote') {
        return mode
    }
    return { ...mode, config_server_url: resolveConfigServerUrl(mode.config_server_url) }
}

export function loadMode(): Mode {
    const modeStr = localStorage.getItem('app_mode')
    if (modeStr) {
        let mode = JSON.parse(modeStr) as Mode
        if (safeOsType() === 'android') {
            return withDefaultConfigServer({ ...mode, mode: 'normal' })
        }
        return withDefaultConfigServer(mode)
    } else {
        return withDefaultConfigServer({ mode: 'normal' })
    }
}

export type Mode = NormalMode | ServiceMode | RemoteMode
