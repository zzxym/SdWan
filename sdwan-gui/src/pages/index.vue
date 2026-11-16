<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { open } from '@tauri-apps/plugin-shell'
import { exit } from '@tauri-apps/plugin-process'
import { I18nUtils, Utils } from "sdwan-frontend-lib"
import type { MenuItem } from 'primevue/menuitem'
import { useTray } from '~/composables/tray'
import { initMobileVpnService } from '~/composables/mobile_vpn'
import { GUIRemoteClient } from '~/modules/api'

import { useToast } from 'primevue'
import { loadMode, saveMode, type Mode } from '~/composables/mode'
import ClientStatusPanel from '~/components/ClientStatusPanel.vue'
import AuthPanel from '~/components/AuthPanel.vue'
import {
  getLocalIpv4,
  getSdwanVersion,
  getServiceStatus,
  initRpcConnection,
  initWebClient,
  isClientRunning,
  sendConfigs,
} from '~/composables/backend'
import { createDefaultSdwanNetworkConfig, resolveConfigServerUrl } from '~/composables/sdwan'
import { isPreviewMode, safeOsType } from '~/composables/preview'

/**
 * SDWAN 精简客户端（只读）
 *
 * - 界面仅用于展示运行状态：未运行，或运行中及其详情
 * - 不提供任何本地配置入口：新建/编辑/删除/启停网络全部由 Web 控制台下发
 * - 客户端启动后仍会注册到配置服务器，使其可被 Web 控制台发现与管控
 */
const { t, locale } = useI18n()
const toast = useToast()
const currentMode = ref<Mode>({ mode: 'normal' })
const isReconnecting = ref(false)
const manualDisconnect = ref(false)

const remoteClient = computed(() => new GUIRemoteClient());
const clientRunning = ref(false);
// 浏览器预览模式：没有 Tauri runtime 时使用演示数据渲染界面
const previewMode = isPreviewMode();

const LOGIN_USER_KEY = 'sdwan_login_user'
const loggedInUser = ref<string>(localStorage.getItem(LOGIN_USER_KEY) || '')

async function initWithMode(mode: Mode) {
  const running_inst_ids = (await remoteClient.value.list_network_instance_ids().catch(() => undefined))?.running_inst_ids ?? []

  if (currentMode.value.mode === 'service' && mode.mode !== 'service') {
    let serviceStatus = await getServiceStatus()
    if (serviceStatus === "Running") {
      manualDisconnect.value = true
      await setServiceStatus(false)
      serviceStatus = await getServiceStatus()
      for (let i = 0; i < 10; i++) { // macOS takes a while to stop the service
        if (serviceStatus === "Stopped") {
          break;
        }
        await new Promise(resolve => setTimeout(resolve, 100))
        serviceStatus = await getServiceStatus()
      }
    }
    if (serviceStatus === "Stopped") {
      await initService(undefined)
    }
  }

  let url: string | undefined = undefined
  let retrys = 1
  switch (mode.mode) {
    case 'remote':
      if (!mode.remote_rpc_address) {
        toast.add({ severity: 'error', summary: t('error'), detail: t('mode.remote_rpc_address_empty'), life: 10000 })
        return initWithMode({ ...mode, mode: 'normal' });
      }
      url = mode.remote_rpc_address
      break;
    case 'service': {
      if (!mode.config_dir || !mode.file_log_dir || !mode.file_log_level || !mode.rpc_portal) {
        toast.add({ severity: 'error', summary: t('error'), detail: t('mode.service_config_empty'), life: 10000 })
        return initWithMode({ ...mode, mode: 'normal' });
      }
      let serviceStatus = await getServiceStatus()
      const coreVersion = await getSdwanVersion()
      if (serviceStatus === "NotInstalled" || mode.installed_core_version !== coreVersion) {
        mode.config_server_url = resolveConfigServerUrl(mode.config_server_url)
        await initService({
          config_dir: mode.config_dir,
          file_log_dir: mode.file_log_dir,
          file_log_level: mode.file_log_level,
          rpc_portal: mode.rpc_portal,
          config_server: mode.config_server_url,
        })
        mode.installed_core_version = coreVersion
        serviceStatus = await getServiceStatus()
      }
      if (serviceStatus === "Stopped") {
        await setServiceStatus(true)
      }
      url = "tcp://" + mode.rpc_portal.replace("0.0.0.0", "127.0.0.1")
      retrys = 5
      break;
    }
    case 'normal':
      url = mode.rpc_portal;
      break;
  }
  for (let i = 0; i < retrys; i++) {
    try {
      await connectRpcClient(mode.mode === 'normal', url)
      break;
    } catch (e) {
      if (i === retrys - 1) {
        const errMsg = e instanceof Error ? e.message : String(e)
        toast.add({
          severity: 'error',
          summary: t('error'),
          detail: t('mode.rpc_connection_failed', { error: errMsg }),
          life: 1000,
        })
        throw e;
      }
      console.error("Error connecting rpc client, retrying...", e)
      await new Promise(resolve => setTimeout(resolve, 1000))
    }
  }
  await sendConfigs(running_inst_ids.map(Utils.UuidToStr))
  // 未登录时不连接配置服务器：登录成功后会以用户名作为 token 重新连接
  if (mode.mode === 'normal' && loggedInUser.value) {
    mode.config_server_url = resolveConfigServerUrl(loggedInUser.value)
    initWebClient(mode.config_server_url)
  }
  currentMode.value = mode
  saveMode(mode)
  clientRunning.value = await isClientRunning()
}

onMounted(async () => {
  const cleanupFns: Array<() => void> = []

  if (safeOsType() === 'android') {
    try {
      await initMobileVpnService()
      console.error("sdwan init vpn service done")
    } catch (e: any) {
      console.error("sdwan init vpn service failed", e)
    }
  }

  try {
    cleanupFns.push(await listenGlobalEvents())
  } catch (e) {
    console.error('Failed to listen global events', e)
  }
  currentMode.value = loadMode()
  if (previewMode) {
    clientRunning.value = true
  }
  else {
    await initWithMode(currentMode.value);
  }

  onUnmounted(() => {
    cleanupFns.forEach(unlisten => unlisten())
  })
});

useTray(true)

// SDWAN: 窗口/任务栏标题固定显示 "SDWAN Gui"
onMounted(async () => {
  try {
    await getCurrentWindow().setTitle("SDWAN Gui")
  } catch (e) {
    console.error("Failed to set window title", e)
  }
})

watch(clientRunning, async (newVal, oldVal) => {
  if (!newVal && oldVal) {
    if (manualDisconnect.value) {
      manualDisconnect.value = false
      return
    }
    await reconnectClient()
  }
})

onMounted(async () => {
  if (previewMode) {
    return
  }
  clientRunning.value = await isClientRunning().catch(() => false)
  const timer = setInterval(async () => {
    try {
      clientRunning.value = await isClientRunning()
    } catch (e) {
      clientRunning.value = false
      console.error("Error checking client running status", e)
    }
  }, 1000)

  onUnmounted(() => {
    clearInterval(timer)
  })
})

async function reconnectClient() {
  if (isReconnecting.value || previewMode) {
    return
  }
  isReconnecting.value = true
  try {
    await initWithMode(loadMode())
  } catch (e: any) {
    console.error("Error reconnecting client", e)
  } finally {
    isReconnecting.value = false
  }
}

/**
 * 登录成功后的受控初始化：
 * - 用用户名作为 token 连接配置服务器 udp://oa.xiaolin.cc:22020/<username>
 * - 创建专属网络：网络名=用户名，网络密码=用户注册的密码，子网代理=本机局域网 IP/32
 * - 复用 load_configs 路径保存并自动运行，不开放任何用户可编辑的配置入口
 */
async function onAuthenticated(credential: { username: string, password: string }) {
  const { username, password } = credential
  localStorage.setItem(LOGIN_USER_KEY, username)
  loggedInUser.value = username

  const url = resolveConfigServerUrl(username)
  const mode = loadMode()
  if (mode.mode !== 'remote') {
    mode.config_server_url = url
  }
  saveMode(mode)
  currentMode.value = mode

  if (previewMode) {
    clientRunning.value = true
    return
  }

  try {
    await initWebClient(url)
  } catch (e: any) {
    console.error('Failed to connect config server after login', e)
  }
  await provisionUserNetwork(username, password)
  clientRunning.value = true
}

async function provisionUserNetwork(username: string, password: string) {
  const cfg = createDefaultSdwanNetworkConfig()
  cfg.network_name = username
  cfg.network_secret = password

  const localIp = await getLocalIpv4().catch(() => '')
  cfg.proxy_cidrs = localIp ? [`${localIp}/32`] : []

  localStorage.setItem('networkList', JSON.stringify([{
    config: cfg,
    source: 'user',
  }]))

  await sendConfigs([cfg.instance_id])
}

async function logout() {
  localStorage.removeItem(LOGIN_USER_KEY)
  localStorage.removeItem('networkList')
  loggedInUser.value = ''
  if (!previewMode) {
    try {
      await initWebClient(undefined)
    } catch (e: any) {
      console.error('Failed to disconnect config server', e)
    }
  }
}

onMounted(async () => {
  window.setTimeout(async () => {
    await setTrayMenu([
      await MenuItemShow(t('tray.show')),
      await MenuItemExit(t('tray.exit')),
    ])
  }, 1000)
})

let current_log_level = 'off'

const log_menu = ref()
// 从后端获取正确的日志路径
async function getLogDirPath(): Promise<string> {
  return await invoke<string>('get_log_dir_path')
}

const log_menu_items_popup: Ref<MenuItem[]> = ref([
  ...['off', 'warn', 'info', 'debug', 'trace'].map(level => ({
    label: () => t(`logging_level_${level}`) + (current_log_level === level ? ' ✓' : ''),
    command: async () => {
      current_log_level = level
      await setLoggingLevel(level)
    },
  })),
  {
    separator: true,
  },
  {
    label: () => t('logging_open_dir'),
    icon: 'pi pi-folder-open',
    command: async () => {
      // console.log('open log dir', await getLogDirPath())
      await open(await getLogDirPath())
    },
    visible: () => safeOsType() !== 'android',
  },
  {
    label: () => t('logging_copy_dir'),
    icon: 'pi pi-tablet',
    command: async () => {
      await writeText(await getLogDirPath())
    },
  },
])

function toggle_log_menu(event: any) {
  log_menu.value.toggle(event)
}

function getLabel(item: MenuItem) {
  return typeof item.label === 'function' ? item.label() : item.label
}

const setting_menu_items: Ref<MenuItem[]> = ref([
  {
    label: () => t('exchange_language'),
    icon: 'pi pi-language',
    command: async () => {
      await I18nUtils.loadLanguageAsync((locale.value === 'en' ? 'cn' : 'en'))
      await setTrayMenu([
        await MenuItemShow(t('tray.show')),
        await MenuItemExit(t('tray.exit')),
      ])
    },
  },
  {
    key: 'logging_menu',
    label: () => t('logging'),
    icon: 'pi pi-file',
    items: [], // Keep this to show it's a parent menu
  },
  {
    label: () => t('auth.logout'),
    icon: 'pi pi-sign-out',
    command: logout,
    visible: () => !!loggedInUser.value,
  },
  {
    label: () => t('exit'),
    icon: 'pi pi-power-off',
    command: async () => {
      await exit(1)
    },
  },
])

async function connectRpcClient(isNormalMode: boolean, url?: string) {
  await initRpcConnection(isNormalMode, url)
  console.log("sdwan rpc connection established, isNormalMode: ", isNormalMode)
}
</script>

<template>
  <div id="root" class="flex flex-col">
    <Menu ref="log_menu" :model="log_menu_items_popup" :popup="true" />

    <AuthPanel v-if="!loggedInUser" :preview-mode="previewMode" @authenticated="onAuthenticated" />
    <ClientStatusPanel v-else-if="clientRunning" class="flex-1 overflow-y-auto" />
    <div v-else class="empty-state flex-1 flex flex-col items-center py-12">
      <i class="pi pi-server text-5xl text-secondary mb-4 opacity-50"></i>
      <div class="text-xl text-center font-medium mb-3">{{ t('client.not_running') }}
      </div>
      <Button @click="reconnectClient" :loading="isReconnecting" :label="t('client.retry')" icon="pi pi-replay"
        iconPos="left" />
    </div>

    <Menubar :model="setting_menu_items" breakpoint="795px">
      <template #item="{ item, props }">
        <a v-if="item.key === 'logging_menu'" v-bind="props.action" @click="toggle_log_menu">
          <span :class="item.icon" />
          <span class="p-menubar-item-label">{{ getLabel(item) }}</span>
          <span class="pi pi-angle-down p-menubar-item-icon text-[9px]"></span>
        </a>
        <a v-else v-bind="props.action">
          <span :class="item.icon" />
          <span class="p-menubar-item-label">{{ getLabel(item) }}</span>
        </a>
      </template>
    </Menubar>
  </div>
</template>

<style scoped lang="postcss">
#root {
  height: 100vh;
  width: 100vw;
}

.p-dropdown :deep(.p-dropdown-panel .p-dropdown-items .p-dropdown-item) {
  padding: 0 0.5rem;
}
</style>

<style>
body {
  height: 100vh;
  width: 100vw;
  padding: 0;
  margin: 0;
  overflow: hidden;
}

.p-menubar .p-menuitem {
  margin: 0;
}

.p-select-overlay {
  max-width: calc(100% - 2rem);
}
</style>
