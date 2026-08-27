pub mod fallback;
pub mod popup;
#[cfg(target_os = "linux")]
pub mod window_icon;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard};

use crate::api::{NotifyRequest, NotifyVia};
use crate::config::Config;

/// 弹窗 GUI 是否可用(启动时探测,失败则全程走系统通知兜底)
pub static POPUP_AVAILABLE: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
struct PendingPopup {
    title: String,
    body_html: String,
}

impl PendingPopup {
    fn new(title: impl Into<String>, body_html: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body_html: body_html.into(),
        }
    }
}

#[derive(Default)]
struct PendingPopupState {
    latest: Option<PendingPopup>,
    scheduled: bool,
}

impl PendingPopupState {
    /// 覆盖待处理通知;返回 true 表示调用方需要安排一次 GUI 刷新
    fn push(&mut self, pending: PendingPopup) -> bool {
        self.latest = Some(pending);
        if self.scheduled {
            false
        } else {
            self.scheduled = true;
            true
        }
    }

    const fn take_latest(&mut self) -> Option<PendingPopup> {
        self.latest.take()
    }

    /// 本轮完成后若又有新通知则继续调度,否则解除已调度状态
    const fn finish_delivery(&mut self) -> bool {
        if self.latest.is_some() {
            true
        } else {
            self.scheduled = false;
            false
        }
    }

    fn cancel_delivery(&mut self) {
        self.latest = None;
        self.scheduled = false;
    }
}

static PENDING_POPUP: LazyLock<Mutex<PendingPopupState>> =
    LazyLock::new(|| Mutex::new(PendingPopupState::default()));

fn pending_popup() -> MutexGuard<'static, PendingPopupState> {
    PENDING_POPUP
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn deliver_latest_popup() {
    let pending = pending_popup().take_latest();
    if let Some(pending) = pending {
        popup::spawn(&pending.title, &pending.body_html, false);
    }

    let mut pending = pending_popup();
    if pending.finish_delivery()
        && let Err(e) = slint::invoke_from_event_loop(deliver_latest_popup)
    {
        pending.cancel_delivery();
        log::warn!("后续弹窗投递失败: {e}");
    }
    drop(pending);
}

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
        let mut pending = pending_popup();
        let needs_schedule = pending.push(PendingPopup::new(title, body_html));
        if !needs_schedule {
            return true;
        }
        let scheduled = slint::invoke_from_event_loop(deliver_latest_popup);
        if scheduled.is_err() {
            pending.cancel_delivery();
        }
        drop(pending);
        match scheduled {
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

#[cfg(test)]
mod tests {
    use super::{PendingPopup, PendingPopupState};

    #[test]
    fn pending_popup_keeps_only_latest_notification() {
        let mut pending = PendingPopupState::default();

        assert!(pending.push(PendingPopup::new("A", "1")));
        assert!(!pending.push(PendingPopup::new("B", "2")));
        assert!(!pending.push(PendingPopup::new("C", "3")));

        let latest = pending.take_latest().unwrap();
        assert_eq!(latest.title, "C");
        assert_eq!(latest.body_html, "3");
        assert!(!pending.finish_delivery());
    }
}
