import { type } from '@tauri-apps/plugin-os';
import { SDWAN_CONFIG_SERVER_URL } from './sdwan';

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


// SDWAN: 未设置配置服务器时，使用内置的预置地址
function withDefaultConfigServer(mode: Mode): Mode {
    if (mode.mode === 'remote') {
        return mode
    }
    if (!mode.config_server_url) {
        return { ...mode, config_server_url: SDWAN_CONFIG_SERVER_URL }
    }
    return mode
}

export function loadMode(): Mode {
    const modeStr = localStorage.getItem('app_mode')
    if (modeStr) {
        let mode = JSON.parse(modeStr) as Mode
        if (type() === 'android') {
            return withDefaultConfigServer({ ...mode, mode: 'normal' })
        }
        return withDefaultConfigServer(mode)
    } else {
        return withDefaultConfigServer({ mode: 'normal' })
    }
}

export type Mode = NormalMode | ServiceMode | RemoteMode
