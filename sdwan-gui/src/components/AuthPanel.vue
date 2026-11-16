<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { getCaptcha, login, register } from '~/composables/auth'

/**
 * SDWAN 客户端登录 / 注册面板
 *
 * 账号体系直接复用 web 控制台（http://oa.xiaolin.cc:11211）。
 * 登录成功后由父组件用该用户名连接配置服务器并创建专属网络。
 */
const props = defineProps<{
  previewMode?: boolean
}>()

const emit = defineEmits<{
  authenticated: [{ username: string, password: string }]
}>()

const { t } = useI18n()

type Tab = 'login' | 'register'

const tab = ref<Tab>('login')
const username = ref('')
const password = ref('')
const registerUsername = ref('')
const registerPassword = ref('')
const captcha = ref('')
const captchaId = ref('')
const captchaImage = ref('')
const loading = ref(false)
const error = ref('')

const MOCK_CAPTCHA = 'data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIxMjAiIGhlaWdodD0iNDAiPjxyZWN0IHdpZHRoPSIxMjAiIGhlaWdodD0iNDAiIGZpbGw9IiNlZGYzZjgiLz48dGV4dCB4PSI2MCIgeT0iMjgiIGZvbnQtc2l6ZT0iMjAiIHRleHQtYW5jaG9yPSJtaWRkbGUiIGZpbGw9IiMzNzQ1NWEiPjEyMzQ8L3RleHQ+PC9zdmc+'

const usernameModel = computed({
  get: () => tab.value === 'login' ? username.value : registerUsername.value,
  set: (val: string) => {
    if (tab.value === 'login') username.value = val
    else registerUsername.value = val
  },
})

const passwordModel = computed({
  get: () => tab.value === 'login' ? password.value : registerPassword.value,
  set: (val: string) => {
    if (tab.value === 'login') password.value = val
    else registerPassword.value = val
  },
})

async function loadCaptcha() {
  if (props.previewMode) {
    captchaId.value = 'preview'
    captchaImage.value = MOCK_CAPTCHA
    return
  }
  try {
    const info = await getCaptcha()
    captchaId.value = info.captcha_id
    captchaImage.value = info.image
  } catch (e) {
    console.error('Failed to load captcha', e)
    captchaId.value = ''
    captchaImage.value = ''
  }
}

function switchTab(next: Tab) {
  tab.value = next
  error.value = ''
}

watch(tab, (next) => {
  if (next === 'register') {
    captcha.value = ''
    loadCaptcha()
  }
})

onMounted(() => {
  loadCaptcha()
})

async function submit() {
  if (loading.value) {
    return
  }
  error.value = ''

  const user = tab.value === 'login' ? username.value.trim() : registerUsername.value.trim()
  const pass = tab.value === 'login' ? password.value : registerPassword.value
  if (!user || !pass) {
    error.value = t('auth.required')
    return
  }

  loading.value = true
  try {
    if (props.previewMode) {
      emit('authenticated', { username: user, password: pass })
      return
    }

    if (tab.value === 'register') {
      const reg = await register(user, pass, captcha.value.trim(), captchaId.value)
      if (!reg.success) {
        error.value = `${t('auth.register_failed')}: ${reg.message}`
        captcha.value = ''
        await loadCaptcha()
        return
      }
    }

    const result = await login(user, pass)
    if (!result.success) {
      error.value = `${t('auth.login_failed')}: ${result.message}`
      return
    }
    emit('authenticated', { username: user, password: pass })
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="flex flex-1 items-center justify-center p-4 overflow-y-auto">
    <Card class="w-full max-w-md">
      <template #title>
        <div class="text-center text-2xl font-semibold">SDWAN</div>
      </template>
      <template #subtitle>
        <div class="text-center text-sm text-surface-500 dark:text-surface-400">{{ t('auth.subtitle') }}</div>
      </template>
      <template #content>
        <div class="flex flex-col gap-4">
          <div class="flex gap-2">
            <Button class="flex-1" :severity="tab === 'login' ? 'primary' : 'secondary'"
              :label="t('auth.login')" @click="switchTab('login')" />
            <Button class="flex-1" :severity="tab === 'register' ? 'primary' : 'secondary'"
              :label="t('auth.register')" @click="switchTab('register')" />
          </div>

          <form class="flex flex-col gap-3" @submit.prevent="submit">
            <div class="flex flex-col gap-1">
              <label class="text-sm font-medium" for="auth-username">{{ t('auth.username') }}</label>
              <input id="auth-username" v-model="usernameModel" type="text"
                autocomplete="username" class="w-full rounded-md border border-surface-300 dark:border-surface-600 bg-transparent px-3 py-2 outline-none focus:border-primary"
                :placeholder="t('auth.username_placeholder')" />
            </div>

            <div class="flex flex-col gap-1">
              <label class="text-sm font-medium" for="auth-password">{{ t('auth.password') }}</label>
              <input id="auth-password" v-model="passwordModel" type="password"
                :autocomplete="tab === 'login' ? 'current-password' : 'new-password'"
                class="w-full rounded-md border border-surface-300 dark:border-surface-600 bg-transparent px-3 py-2 outline-none focus:border-primary"
                :placeholder="t('auth.password_placeholder')" />
            </div>

            <div v-if="tab === 'register'" class="flex flex-col gap-1">
              <label class="text-sm font-medium" for="auth-captcha">{{ t('auth.captcha') }}</label>
              <div class="flex gap-2">
                <input id="auth-captcha" v-model="captcha" type="text" class="flex-1 rounded-md border border-surface-300 dark:border-surface-600 bg-transparent px-3 py-2 outline-none focus:border-primary"
                  :placeholder="t('auth.captcha_placeholder')" />
                <img :src="captchaImage" :alt="t('auth.captcha')" class="h-10 w-auto rounded border border-surface-300 cursor-pointer"
                  :title="t('auth.captcha_refresh')" @click="loadCaptcha" />
              </div>
            </div>

            <div v-if="error" class="rounded-md bg-red-50 dark:bg-red-950 px-3 py-2 text-sm text-red-600 dark:text-red-300">
              {{ error }}
            </div>

            <Button type="submit" class="w-full" :label="tab === 'login' ? t('auth.login_button') : t('auth.register_button')"
              :loading="loading" />

            <div class="text-xs text-surface-500 dark:text-surface-400 text-center">
              {{ tab === 'login' ? t('auth.login_hint') : t('auth.register_hint') }}
            </div>
          </form>
        </div>
      </template>
    </Card>
  </div>
</template>
