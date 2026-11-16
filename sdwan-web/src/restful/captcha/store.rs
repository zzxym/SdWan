//! 验证码存储：把验证码答案与一个随机 `captcha_id` 绑定。
//!
//! 注册时前端把 `captcha_id` 与用户输入的验证码一起提交，服务端据此校验，
//! **不再依赖浏览器的 session cookie**。这样即使控制台被跨站嵌入
//! （如通过 LuCI 的 iframe 访问、https 页面引用 http 接口、浏览器拦截第三方
//! Cookie 等），SameSite 策略导致 cookie 无法送达时，注册流程依然可用。
//!
//! 验证码是一次性的：无论校验成功与否，条目都会被消费，避免暴力破解。

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::time::{Duration, Instant};

use uuid::Uuid;

/// 验证码有效期
const CAPTCHA_TTL: Duration = Duration::from_secs(300);

/// 单个验证码条目
#[derive(Debug)]
struct CaptchaEntry {
    /// 正确答案
    answer: String,
    /// 过期时间
    expires_at: Instant,
}

/// 验证码存储：`captcha_id` -> 正确答案
#[derive(Debug, Default)]
pub struct CaptchaStore {
    entries: Mutex<HashMap<String, CaptchaEntry>>,
}

impl CaptchaStore {
    /// 登记一个验证码答案，返回对应的 `captcha_id`
    pub fn issue(&self, answer: &str) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Instant::now();

        let mut guard = self.lock();
        // 顺带清理过期条目，避免长时间运行时无限增长
        guard.retain(|_, entry| entry.expires_at > now);
        guard.insert(
            id.clone(),
            CaptchaEntry {
                answer: answer.to_string(),
                expires_at: now + CAPTCHA_TTL,
            },
        );

        id
    }

    /// 校验验证码。校验后该验证码立即失效（一次性使用）。
    ///
    /// 返回 `true` 表示答案正确且未过期。
    pub fn verify(&self, captcha_id: &str, input: &str) -> bool {
        let now = Instant::now();

        let mut guard = self.lock();
        let Some(entry) = guard.remove(captcha_id) else {
            // 未知或已被消费的验证码
            return false;
        };

        if entry.expires_at <= now {
            return false;
        }

        entry.answer.eq_ignore_ascii_case(input.trim())
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, CaptchaEntry>> {
        // 锁中毒时恢复数据，避免整个服务不可用
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_matches_case_insensitively() {
        let store = CaptchaStore::default();
        let id = store.issue("qebv");
        assert!(store.verify(&id, "QEBV"));
        // 一次性使用，第二次应失败
        assert!(!store.verify(&id, "qebv"));
    }

    #[test]
    fn verify_rejects_unknown_id() {
        let store = CaptchaStore::default();
        assert!(!store.verify("not-exist", "qebv"));
    }

    #[test]
    fn verify_rejects_wrong_answer() {
        let store = CaptchaStore::default();
        let id = store.issue("qebv");
        assert!(!store.verify(&id, "qebw"));
    }
}
