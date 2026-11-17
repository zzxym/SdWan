<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Utils } from 'sdwan-frontend-lib'
import type { NetworkTypes } from 'sdwan-frontend-lib'
import {
  collectNetworkInfo,
  isClientRunning,
  isWebClientConnected,
  listNetworkInstanceIds,
} from '~/composables/backend'
import { isPreviewMode, PREVIEW_INSTANCES, previewNetworkInfo } from '~/composables/preview'

/**
 * SDWAN 精简客户端状态面板（只读）
 *
 * 该客户端只运行当前登录用户对应的一个网络，本面板仅展示运行状态：
 * - 客户端是否运行、是否已连接服务器
 * - 当前网络的详情（本机节点信息、对端、延迟、流量等）
 *
 * 面板不提供任何配置或实例选择入口。
 */
const { t } = useI18n()

const clientRunning = ref(false)
const webConnected = ref(false)
const currentInfo = ref<NetworkTypes.NetworkInstance | null>(null)

const REFRESH_INTERVAL_MS = 2000
let timer: ReturnType<typeof setInterval> | undefined

async function loadDetail(uuid: string) {
  if (isPreviewMode()) {
    const previewInfo = previewNetworkInfo()
    currentInfo.value = {
      instance_id: uuid,
      running: previewInfo.running,
      error_msg: '',
      detail: previewInfo,
    } as NetworkTypes.NetworkInstance
    return
  }

  const resp = await collectNetworkInfo(uuid).catch(() => undefined)
  const info = resp?.info?.map?.[uuid]
  currentInfo.value = {
    instance_id: uuid,
    running: info?.running ?? false,
    error_msg: info?.error_msg ?? '',
    detail: info,
  } as NetworkTypes.NetworkInstance
}

async function refresh() {
  if (isPreviewMode()) {
    clientRunning.value = true
    webConnected.value = true
    await loadDetail(PREVIEW_INSTANCES[0]?.uuid ?? '')
    return
  }

  clientRunning.value = await isClientRunning().catch(() => false)
  webConnected.value = await isWebClientConnected().catch(() => false)

  if (!clientRunning.value) {
    currentInfo.value = null
    return
  }

  try {
    const idsResp = await listNetworkInstanceIds()
    const runningId = (idsResp?.running_inst_ids ?? []).map(Utils.UuidToStr)[0]
    if (runningId) {
      await loadDetail(runningId)
    } else {
      currentInfo.value = null
    }
  }
  catch (e) {
    console.error('Failed to refresh client status', e)
  }
}

onMounted(async () => {
  await refresh()
  timer = setInterval(refresh, REFRESH_INTERVAL_MS)
})

onUnmounted(() => {
  if (timer) {
    clearInterval(timer)
  }
})
</script>

<template>
  <div class="flex flex-col gap-3 p-3 overflow-y-auto">
    <Card>
      <template #title>
        {{ t('client.status_title') }}
      </template>
      <template #content>
        <div class="grid grid-cols-2 gap-4">
          <div class="flex flex-col gap-1">
            <span class="text-sm text-surface-500 dark:text-surface-400">{{ t('client.status_client') }}</span>
            <span class="flex items-center gap-2 font-medium">
              <i class="pi pi-circle-fill text-xs" :class="clientRunning ? 'text-green-500' : 'text-red-500'" />
              {{ clientRunning ? t('client.status_running') : t('client.status_stopped') }}
            </span>
          </div>
          <div class="flex flex-col gap-1">
            <span class="text-sm text-surface-500 dark:text-surface-400">{{ t('client.status_web_console') }}</span>
            <span class="flex items-center gap-2 font-medium">
              <i class="pi pi-circle-fill text-xs" :class="webConnected ? 'text-green-500' : 'text-orange-500'" />
              {{ webConnected ? t('client.status_web_connected') : t('client.status_web_disconnected') }}
            </span>
          </div>
        </div>
      </template>
    </Card>

    <Status v-if="currentInfo" :cur-network-inst="currentInfo" :compact="true" />
  </div>
</template>
