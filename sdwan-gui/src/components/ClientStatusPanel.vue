<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Utils } from 'sdwan-frontend-lib'
import type { NetworkTypes } from 'sdwan-frontend-lib'
import {
  collectNetworkInfo,
  getNetworkMetas,
  getSdwanVersion,
  isClientRunning,
  isWebClientConnected,
  listNetworkInstanceIds,
} from '~/composables/backend'
import { isPreviewMode, PREVIEW_INSTANCES, previewNetworkInfo } from '~/composables/preview'

/**
 * SDWAN 精简客户端状态面板（只读）
 *
 * 该客户端的所有网络配置均由 Web 控制台下发，本面板只负责展示运行状态：
 * - 客户端是否运行、是否已注册到 Web 控制台
 * - 当前运行中的网络实例及其详情（本机节点信息、对端、延迟、流量等）
 *
 * 面板不提供任何配置入口。
 */
const { t } = useI18n()

interface InstanceEntry {
  uuid: string
  name: string
  running: boolean
}

const clientRunning = ref(false)
const webConnected = ref(false)
const version = ref('')
const instances = ref<InstanceEntry[]>([])
const selectedId = ref<string | undefined>(undefined)
const currentInfo = ref<NetworkTypes.NetworkInstance | null>(null)

const REFRESH_INTERVAL_MS = 2000
let timer: ReturnType<typeof setInterval> | undefined

const runningCount = computed(() => instances.value.filter(i => i.running).length)

const instanceOptions = computed(() => instances.value.map(i => ({
  value: i.uuid,
  label: i.running ? i.name : `${i.name} (${t('client.status_stopped')})`,
})))

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
    version.value = '2.6.4'
    instances.value = PREVIEW_INSTANCES.map(i => ({ uuid: i.uuid, name: i.name, running: i.running }))
    if (!selectedId.value) {
      selectedId.value = PREVIEW_INSTANCES[0]?.uuid
    }
    if (selectedId.value) {
      await loadDetail(selectedId.value)
    }
    return
  }

  clientRunning.value = await isClientRunning().catch(() => false)
  webConnected.value = await isWebClientConnected().catch(() => false)
  version.value = await getSdwanVersion().catch(() => '')

  if (!clientRunning.value) {
    instances.value = []
    currentInfo.value = null
    selectedId.value = undefined
    return
  }

  try {
    const idsResp = await listNetworkInstanceIds()
    const runningIds = (idsResp?.running_inst_ids ?? []).map(Utils.UuidToStr)
    const disabledIds = (idsResp?.disabled_inst_ids ?? []).map(Utils.UuidToStr)
    const metas = await getNetworkMetas([...runningIds, ...disabledIds]).catch(() => undefined)

    instances.value = [
      ...runningIds.map(uuid => ({
        uuid,
        name: metas?.metas?.[uuid]?.network_name || uuid,
        running: true,
      })),
      ...disabledIds.map(uuid => ({
        uuid,
        name: metas?.metas?.[uuid]?.network_name || uuid,
        running: false,
      })),
    ]

    if (!selectedId.value || !instances.value.some(i => i.uuid === selectedId.value)) {
      selectedId.value = instances.value.find(i => i.running)?.uuid ?? instances.value[0]?.uuid
    }

    if (selectedId.value) {
      await loadDetail(selectedId.value)
    } else {
      currentInfo.value = null
    }
  }
  catch (e) {
    console.error('Failed to refresh client status', e)
  }
}

watch(selectedId, async (val) => {
  if (val) {
    await loadDetail(val)
  }
  else {
    currentInfo.value = null
  }
})

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
        <div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
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
          <div class="flex flex-col gap-1">
            <span class="text-sm text-surface-500 dark:text-surface-400">{{ t('client.status_version') }}</span>
            <span class="font-medium">{{ version || '-' }}</span>
          </div>
          <div class="flex flex-col gap-1">
            <span class="text-sm text-surface-500 dark:text-surface-400">{{ t('client.status_instance_count') }}</span>
            <span class="font-medium">{{ runningCount }} / {{ instances.length }}</span>
          </div>
        </div>
      </template>
    </Card>

    <Card v-if="instances.length > 0">
      <template #title>
        {{ t('client.status_select_instance') }}
      </template>
      <template #content>
        <Select v-model="selectedId" :options="instanceOptions" option-label="label" option-value="value"
          class="w-full" />
      </template>
    </Card>

    <Card v-else>
      <template #title>
        {{ t('client.status_instance_title') }}
      </template>
      <template #content>
        <div class="flex flex-col items-center gap-2 py-4 text-surface-500 dark:text-surface-400">
          <i class="pi pi-inbox text-3xl opacity-50" />
          <span>{{ t('client.status_no_instance') }}</span>
        </div>
      </template>
    </Card>

    <Status v-if="currentInfo" :cur-network-inst="currentInfo" />
  </div>
</template>
