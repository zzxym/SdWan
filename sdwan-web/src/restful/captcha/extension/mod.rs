pub mod axum_tower_sessions;

use super::base::captcha::AbstractCaptcha;
use super::builder::spec::SpecCaptcha;
use super::{CaptchaFont, NewCaptcha};

/// 验证码工具类 - Captcha Utils
///
/// 默认使用[SpecCaptcha]（静态PNG字母验证码）作为验证码实现，用户也可以指定其他实现了[AbstractCaptcha]的类型。
///
/// Use [SpecCaptcha] (static PNG-format alphabetical Captcha) as the default implement of the Captcha service. Users may use other implementation of [AbstractCaptcha] they prefer.
///
pub struct CaptchaUtil<T: AbstractCaptcha = SpecCaptcha> {
    captcha_instance: T,
}

impl<T: AbstractCaptcha> NewCaptcha for CaptchaUtil<T> {
    fn new() -> Self {
        Self {
            captcha_instance: T::new(),
        }
    }

    fn with_size(width: i32, height: i32) -> Self {
        Self {
            captcha_instance: T::with_size(width, height),
        }
    }

    fn with_size_and_len(width: i32, height: i32, len: usize) -> Self {
        Self {
            captcha_instance: T::with_size_and_len(width, height, len),
        }
    }

    fn with_all(width: i32, height: i32, len: usize, font: CaptchaFont, font_size: f32) -> Self {
        Self {
            captcha_instance: T::with_all(width, height, len, font, font_size),
        }
    }
}

impl<T: AbstractCaptcha> CaptchaUtil<T> {
    /// 获取当前验证码的答案文本。
    ///
    /// 若尚未生成验证码，会先生成一份，因此调用顺序应为：先渲染图片（`out`/`base64`），
    /// 再取答案，两者指向同一份验证码。
    pub fn answer(&mut self) -> String {
        self.captcha_instance.get_chars().iter().collect()
    }

    /// 输出 Base64 编码的验证码图片（带 `data:image/png;base64,` 前缀）
    pub fn base64(&mut self) -> Result<String, T::Error> {
        self.captcha_instance.base64()
    }
}
