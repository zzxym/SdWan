import { Md5 } from 'ts-md5'
import { I18nUtils } from 'sdwan-frontend-lib'

/**
 * Web 控制台（sdwan-web）认证接口封装。
 *
 * 服务端 CORS 为 very_permissive，因此可从 Tauri WebView 直接跨域调用。
 * 登录接口只用于校验账号密码：session cookie 是 HttpOnly + SameSite=Lax，
 * GUI 不依赖它维持会话，而是用用户名作为配置服务器的 token 建立 UDP 连接。
 */
export const AUTH_API_BASE = 'https://sdwanlogin.xiaolin.cc:81'

export interface CaptchaInfo {
  captcha_id: string
  image: string
  expires_in?: number
}

export interface AuthResult {
  success: boolean
  message: string
}

async function requestJson<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${AUTH_API_BASE}${path}`, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers ?? {}),
    },
  })

  if (!response.ok) {
    let detail = `${response.status} ${response.statusText}`
    try {
      const body = await response.json()
      if (typeof body === 'string') {
        detail = body
      } else if (body && typeof body === 'object') {
        detail = String((body as Record<string, unknown>).detail
          ?? (body as Record<string, unknown>).message
          ?? JSON.stringify(body))
      }
    } catch {
      // 响应不是 JSON 时保留 HTTP 状态文本
    }
    throw new Error(detail)
  }

  return await response.json() as T
}

export async function getCaptcha(): Promise<CaptchaInfo> {
  return await requestJson<CaptchaInfo>('/api/v1/auth/captcha_info')
}

/**
 * 注册账号。密码与 Web 控制台保持一致：先 MD5 再提交。
 */
export async function register(
  username: string,
  password: string,
  captcha: string,
  captchaId: string,
): Promise<AuthResult> {
  try {
    await requestJson('/api/v1/auth/register', {
      method: 'POST',
      body: JSON.stringify({
        credentials: { username, password: Md5.hashStr(password) },
        captcha,
        captcha_id: captchaId,
      }),
    })
    return { success: true, message: '' }
  } catch (e) {
    return { success: false, message: e instanceof Error ? e.message : String(e) }
  }
}

/**
 * 校验账号密码。返回成功即表示可以用该用户名连接配置服务器。
 */
export async function login(username: string, password: string): Promise<AuthResult> {
  try {
    await requestJson('/api/v1/auth/login', {
      method: 'POST',
      body: JSON.stringify({
        username,
        password: Md5.hashStr(password),
      }),
    })
    return { success: true, message: '' }
  } catch (e) {
    return { success: false, message: localizeAuthError(e) }
  }
}

/**
 * 把服务端返回的账号状态原因映射为本地化文案。
 *
 * 服务端在账号被禁用或过期时返回 403，并带上英文原因，这里翻译成界面语言。
 */
function localizeAuthError(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e)
  const t = I18nUtils.i18n.global.t as (key: string) => string
  if (raw.includes('disabled by administrator')) {
    return t('auth.account_disabled')
  }
  if (raw.includes('has expired')) {
    return t('auth.account_expired')
  }
  return raw
}
