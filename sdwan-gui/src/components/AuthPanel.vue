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
  <div class="auth-page flex flex-1 w-full items-center justify-center overflow-y-auto p-4">
    <Card class="auth-card w-full">
      <template #title>
        <div class="auth-title">{{ tab === 'login' ? t('auth.login') : t('auth.register') }}</div>
      </template>
      <template #content>
        <form
          class="auth-form flex flex-col w-full"
          @submit.prevent="submit"
        >
          <div class="flex flex-col gap-2">
            <label class="auth-label" for="auth-username">{{ t('auth.username') }}</label>
            <input
              id="auth-username"
              v-model="usernameModel"
              type="text"
              class="auth-input"
              autocomplete="username"
              :placeholder="t('auth.username_placeholder')"
            >
          </div>

          <div class="flex flex-col gap-2">
            <label class="auth-label" for="auth-password">{{ t('auth.password') }}</label>
            <input
              id="auth-password"
              v-model="passwordModel"
              type="password"
              class="auth-input"
              :autocomplete="tab === 'login' ? 'current-password' : 'new-password'"
              :placeholder="t('auth.password_placeholder')"
            >
          </div>

          <div v-if="tab === 'register'" class="flex flex-col gap-2">
            <label class="auth-label" for="auth-captcha">{{ t('auth.captcha') }}</label>
            <div class="flex gap-3">
              <input
                id="auth-captcha"
                v-model="captcha"
                type="text"
                class="auth-input flex-1"
                :placeholder="t('auth.captcha_placeholder')"
              >
              <img
                :src="captchaImage"
                :alt="t('auth.captcha')"
                class="auth-captcha-img"
                :title="t('auth.captcha_refresh')"
                @click="loadCaptcha"
              >
            </div>
          </div>

          <div v-if="error" class="auth-error">
            {{ error }}
          </div>

          <Button
            type="submit"
            class="auth-submit w-full"
            :label="tab === 'login' ? t('auth.login_button') : t('auth.register_button')"
            :loading="loading"
          />
          <Button
            type="button"
            class="auth-switch w-full"
            severity="secondary"
            :label="tab === 'login' ? t('auth.register') : t('auth.login')"
            @click="switchTab(tab === 'login' ? 'register' : 'login')"
          />
        </form>
      </template>
    </Card>
  </div>
</template>

<style scoped lang="postcss">
.auth-page {
  min-height: 0;
}

.auth-card {
  max-width: 27rem;
  flex: 0 0 auto;
  background: transparent;
  border: none;
  box-shadow: none;
}

.auth-card :deep(.p-card-body) {
  box-sizing: border-box;
  padding: 2.5rem;
}

.auth-card :deep(.p-card-title) {
  text-align: center;
  margin-bottom: 1.75rem;
}

.auth-card :deep(.p-card-content) {
  padding: 0;
}

.auth-title {
  font-size: 1.75rem;
  font-weight: 700;
  text-align: center;
}

.auth-form {
  row-gap: 1.4rem;
}

.auth-label {
  font-size: 1rem;
  font-weight: 600;
}

.auth-input {
  width: 100%;
  min-height: 3.25rem;
  padding: 0.8rem 1rem;
  font-size: 1.05rem;
  color: inherit;
  background: transparent;
  border: 1px solid var(--p-surface-300);
  border-radius: 0.5rem;
  outline: none;
}

.auth-input:focus {
  border-color: var(--p-primary-color);
  box-shadow: 0 0 0 2px rgba(16, 185, 129, 0.18);
}

.auth-captcha-img {
  height: 3.25rem;
  min-width: 8rem;
  object-fit: contain;
  border: 1px solid var(--p-surface-300);
  border-radius: 0.5rem;
  cursor: pointer;
}

.auth-error {
  padding: 0.75rem 1rem;
  font-size: 0.95rem;
  color: #b91c1c;
  background: #fef2f2;
  border-radius: 0.5rem;
}

.auth-submit {
  min-height: 3.25rem;
  font-size: 1.1rem;
}

.auth-switch {
  min-height: 3.25rem;
  font-size: 1.1rem;
}

@media (max-width: 640px) {
  .auth-card :deep(.p-card-body) {
    padding: 1.75rem;
  }
}
</style>
