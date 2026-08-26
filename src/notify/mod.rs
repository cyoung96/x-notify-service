pub mod fallback;
pub mod popup;
#[cfg(target_os = "linux")]
pub mod window_icon;

use std::sync::atomic::{AtomicBool, Ordering};

use crate::api::{NotifyRequest, NotifyVia};
use crate::config::Config;

/// 弹窗 GUI 是否可用(启动时探测,失败则全程走系统通知兜底)
pub static POPUP_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// 通知展示渠道:实现方负责把一条通知真正呈现给用户
pub trait Presenter {
    /// 展示通知;返回 false 表示本渠道投递失败(调用方降级到下一渠道)
    fn present(&self, title: &str, body_html: &str) -> bool;
}

/// 右下角置顶弹窗(主渠道):经事件循环投递给 GUI 线程
pub struct PopupPresenter;

impl Presenter for PopupPresenter {
    fn present(&self, title: &str, body_html: &str) -> bool {
        if !POPUP_AVAILABLE.load(Ordering::Relaxed) {
            return false;
        }
        let title = title.to_string();
        let body = body_html.to_string();
        match slint::invoke_from_event_loop(move || popup::spawn(&title, &body, false)) {
            Ok(()) => true,
            Err(e) => {
                log::warn!("弹窗投递失败,降级系统通知: {e}");
                false
            }
        }
    }
}

/// 投递一条通知:弹窗为主,失败自动降级系统通知兜底。
pub fn dispatch(cfg: &Config, req: &NotifyRequest) -> NotifyVia {
    let title = req.title.trim();
    let body = req.body.as_deref().unwrap_or("");
    if PopupPresenter.present(title, body) {
        return NotifyVia::Popup;
    }
    fallback::SystemPresenter::new(cfg).present(title, body);
    NotifyVia::System
}
