use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{html, PopupWindow};

use slint::ComponentHandle;

/// 弹窗固定逻辑尺寸(与 ui/popup.slint 保持一致),白色圆角卡片
const W_LOGICAL: f64 = 367.0;
const H_LOGICAL: f64 = 206.0;
const MARGIN: f64 = 14.0;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

struct Entry {
    id: u64,
    popup: PopupWindow,
}

// 仅在事件循环线程(GUI 主线程)经 invoke_from_event_loop 访问;不堆叠,同时最多一条
thread_local! {
    static CURRENT: RefCell<Option<Entry>> = const { RefCell::new(None) };
}

/// 启动时探测弹窗 GUI 是否可用(必须在主线程调用,run_event_loop 之前)
pub fn gui_probe() -> bool {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            log::warn!("无 DISPLAY/WAYLAND_DISPLAY,弹窗不可用");
            return false;
        }
    }
    let probe = std::panic::catch_unwind(|| PopupWindow::new().map(drop));
    match probe {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            log::warn!("GUI 初始化失败,弹窗不可用: {e}");
            false
        }
        Err(_) => {
            log::warn!("GUI 初始化 panic,弹窗不可用");
            false
        }
    }
}

/// 展示一条弹窗(只应在事件循环线程调用)。
/// 弹窗常驻不超时:仅点击关闭,或被新通知顶掉。
pub fn spawn(title: String, body_html: String) {
    let Some(area) = crate::screen::work_area() else {
        log::warn!("无法获取屏幕工作区,本条通知走系统通知");
        crate::notify::fallback::show_raw(&title, &html::to_plain_text(&body_html));
        return;
    };

    let popup = match PopupWindow::new() {
        Ok(p) => p,
        Err(e) => {
            log::warn!("弹窗创建失败,本条通知走系统通知: {e}");
            crate::notify::fallback::show_raw(&title, &html::to_plain_text(&body_html));
            return;
        }
    };
    popup.set_notif_title(slint::SharedString::from(title.as_str()));
    set_body(&popup, &body_html);
    // 先定位再显示,避免窗口先出现在默认位置再跳到右下角
    position(&popup, &area);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    popup.on_close_requested(move || remove(id));
    if let Err(e) = popup.show() {
        log::warn!("弹窗显示失败,本条通知走系统通知: {e}");
        crate::notify::fallback::show_raw(&title, &html::to_plain_text(&body_html));
        return;
    }

    CURRENT.with(|current| {
        let mut current = current.borrow_mut();
        if let Some(old) = current.take() {
            let _ = old.popup.window().hide();
            log::debug!("新通知顶掉旧通知");
        }
        // show 后再校一次位置,防止窗口管理器在显示时重排
        position(&popup, &area);
        *current = Some(Entry { id, popup });
    });
}

fn remove(id: u64) {
    CURRENT.with(|current| {
        let mut current = current.borrow_mut();
        let is_current = current.as_ref().is_some_and(|e| e.id == id);
        if is_current
            && let Some(entry) = current.take() {
                let _ = entry.popup.window().hide();
            }
    });
}

/// 关闭当前弹窗(幂等:无弹窗时为空操作)。只应在事件循环线程调用。
pub fn close_current() {
    CURRENT.with(|current| {
        if let Some(entry) = current.borrow_mut().take() {
            let _ = entry.popup.window().hide();
            log::debug!("弹窗被显式关闭");
        }
    });
}

/// 定位到屏幕工作区右下角(任务栏/dock 上方)。
/// area 坐标已统一物理像素;弹窗逻辑尺寸按屏幕 scale 换算。
fn position(popup: &PopupWindow, area: &crate::screen::WorkArea) {
    let win = popup.window();
    let scale = area.scale.max(1.0);
    let w = W_LOGICAL * scale;
    let h = H_LOGICAL * scale;
    let x = area.x + area.w - w - MARGIN * scale;
    let y = area.y + area.h - h - MARGIN * scale;
    log::debug!("弹窗定位: area=({},{},{}x{}) scale={scale} pos=({x},{y})", area.x, area.y, area.w, area.h);
    win.set_position(slint::PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// 解析 HTML 子集并逐行填充正文(行距/字号按行生效)
fn set_body(popup: &PopupWindow, body_html: &str) {
    let parsed = html::parse(body_html);
    let lines: Vec<crate::BodyLine> = html::to_styled_lines(&parsed)
        .into_iter()
        .map(|(markup, size)| crate::BodyLine {
            text: slint::StyledText::from_markdown(&markup).unwrap_or_else(|e| {
                log::warn!("正文行解析失败,按纯文本渲染: {e}");
                slint::StyledText::from_plain_text(&markup)
            }),
            size: size.unwrap_or(html::BASE_FONT_SIZE) as f32,
        })
        .collect();
    popup.set_body_lines(slint::ModelRc::new(slint::VecModel::from(lines)));
}
