use std::cell::RefCell;
use std::sync::OnceLock;

use crate::{PopupWindow, html};

use slint::ComponentHandle as _;

/// 弹窗固定逻辑尺寸(与 ui/popup.slint 保持一致),白色圆角卡片
const W_LOGICAL: f64 = 367.0;
const H_LOGICAL: f64 = 206.0;
const MARGIN: f64 = 14.0;

fn get_or_try_init<T, E>(
    slot: &mut Option<T>,
    init: impl FnOnce() -> Result<T, E>,
) -> Result<(&mut T, bool), E> {
    match slot {
        Some(value) => Ok((value, false)),
        empty @ None => Ok((empty.insert(init()?), true)),
    }
}

struct Entry {
    popup: PopupWindow,
    // 新通知替换这组定时器,旧复校任务随 Timer drop 取消
    fixups: Vec<slint::Timer>,
    /// 弹窗关闭后退出事件循环(notify 子命令单发进程用;服务模式恒 false)
    quit_on_close: bool,
}

// 仅在事件循环线程(GUI 主线程)经 invoke_from_event_loop 访问;进程内始终复用同一窗口
thread_local! {
    static CURRENT: RefCell<Option<Entry>> = const { RefCell::new(None) };
}

static BACKEND_READY: OnceLock<bool> = OnceLock::new();

#[cfg(windows)]
fn window_attributes(
    attributes: winit::window::WindowAttributes,
) -> winit::window::WindowAttributes {
    use winit::platform::windows::WindowAttributesExtWindows as _;
    attributes
        .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
        .with_skip_taskbar(true)
}

#[cfg(target_os = "linux")]
fn window_attributes(
    attributes: winit::window::WindowAttributes,
) -> winit::window::WindowAttributes {
    use winit::platform::x11::{WindowAttributesExtX11 as _, WindowType};
    attributes
        .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
        .with_x11_window_type(vec![WindowType::Notification])
}

#[cfg(not(any(windows, target_os = "linux")))]
const fn window_attributes(
    attributes: winit::window::WindowAttributes,
) -> winit::window::WindowAttributes {
    attributes
}

fn select_backend() -> bool {
    *BACKEND_READY.get_or_init(|| {
        let selected = i_slint_backend_selector::api::BackendSelector::new()
            .backend_name("winit".into())
            .with_winit_window_attributes_hook(window_attributes)
            .select();
        match selected {
            Ok(()) => true,
            Err(e) => {
                log::warn!("GUI 后端初始化失败: {e}");
                false
            }
        }
    })
}

/// 启动时探测弹窗 GUI 是否可用(必须在主线程调用,`run_event_loop` 之前)
pub fn gui_probe() -> bool {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            log::warn!("无 DISPLAY/WAYLAND_DISPLAY,弹窗不可用");
            return false;
        }
    }
    if !select_backend() {
        return false;
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
/// `quit_on_close`:弹窗关闭后退出事件循环(仅 notify 子命令单发进程传 true)。
pub fn spawn(title: &str, body_html: &str, quit_on_close: bool) {
    let Some(area) = crate::screen::work_area() else {
        log::warn!("无法获取屏幕工作区,本条通知走系统通知");
        crate::notify::fallback::show_raw(title, &html::to_plain_text(body_html));
        return;
    };

    let shown = CURRENT.with(|current| {
        let mut current = current.borrow_mut();
        let (entry, created) = get_or_try_init(&mut current, || {
            let popup = PopupWindow::new()?;
            popup.on_close_requested(hide_current);
            Ok::<_, slint::PlatformError>(Entry {
                popup,
                fixups: Vec::new(),
                quit_on_close,
            })
        })?;
        entry.quit_on_close = quit_on_close;
        entry
            .popup
            .set_notif_title(slint::SharedString::from(title));
        set_body(&entry.popup, body_html);
        // 先定位再显示,避免窗口先出现在默认位置再跳到右下角
        position(&entry.popup, &area);
        entry.popup.show()?;

        // 原生窗口在首次 show 后存在;同一 PopupWindow/原生窗口后续只更新内容
        if created {
            #[cfg(target_os = "linux")]
            crate::notify::window_icon::set();
        }
        // 已映射后再切置顶:触发映射后的 ClientMessage,WM 才会应答
        entry.popup.set_raise_above(true);
        entry.fixups = fixup_timers(&entry.popup, area);

        let (px, py) = landing(&area);
        log::info!(
            "弹窗定位: 工作区({},{},{}x{}) → ({px},{py})",
            area.x,
            area.y,
            area.w,
            area.h
        );
        Ok::<_, slint::PlatformError>(())
    });
    if let Err(e) = shown {
        log::warn!("弹窗显示失败,本条通知走系统通知: {e}");
        crate::notify::fallback::show_raw(title, &html::to_plain_text(body_html));
    }
}

/// 显示后多次复校位置,对抗 WM 重摆(部分窗口管理器会把新映射窗口摆到默认位);
/// 每次触发先对比现位置,发现被移动则告警留痕再复校。
/// 返回的定时器由 Entry 持有,下一条通知会整体替换。
fn fixup_timers(popup: &PopupWindow, area: crate::screen::WorkArea) -> Vec<slint::Timer> {
    let weak = popup.as_weak();
    let mut fixups = Vec::with_capacity(4);
    for delay_ms in [50u64, 120, 250, 500] {
        let weak = weak.clone();
        let t = slint::Timer::default();
        t.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(delay_ms),
            move || {
                if let Some(p) = weak.upgrade() {
                    let expected = landing(&area);
                    let cur = p.window().position();
                    if (cur.x - expected.0).abs() > 2i32 || (cur.y - expected.1).abs() > 2i32 {
                        log::warn!("WM 重摆了弹窗(现 {cur:?}),复校回 {expected:?}");
                    }
                    position(&p, &area);
                    if delay_ms == 50 {
                        // 首次 tick 兜底:spawn 后原生窗口尚未可查时重试一次
                        #[cfg(target_os = "linux")]
                        crate::notify::window_icon::set();
                    }
                }
            },
        );
        fixups.push(t);
    }
    fixups
}

/// 隐藏当前窗口;`quit_on_close` 条目在关闭后请求退出事件循环
fn hide_entry(entry: &Entry) {
    let quit = entry.quit_on_close;
    let _ = entry.popup.window().hide();
    if quit {
        let _ = slint::quit_event_loop();
    }
}

fn hide_current() {
    CURRENT.with(|current| {
        if let Some(entry) = current.borrow().as_ref() {
            hide_entry(entry);
        }
    });
}

/// 关闭当前弹窗(幂等:无弹窗时为空操作)。只应在事件循环线程调用。
pub fn close_current() {
    CURRENT.with(|current| {
        if let Some(entry) = current.borrow().as_ref() {
            hide_entry(entry);
            log::debug!("弹窗被显式关闭");
        }
    });
}

/// 定位到屏幕工作区右下角(任务栏/dock 上方)。
/// area 坐标已统一物理像素;弹窗逻辑尺寸按屏幕 scale 换算。
fn position(popup: &PopupWindow, area: &crate::screen::WorkArea) {
    let (x, y) = landing(area);
    log::debug!(
        "弹窗定位: area=({},{},{}x{}) pos=({x},{y})",
        area.x,
        area.y,
        area.w,
        area.h
    );
    popup
        .window()
        .set_position(slint::PhysicalPosition::new(x, y));
}

/// 工作区右下角落点(物理像素,已扣除边距)。
/// pub 供 info 子命令展示:诊断值与弹窗实际定位共用同一计算
// round 后截断为整型像素,值域受屏幕尺寸约束,截断即取整语义
#[allow(clippy::cast_possible_truncation)]
pub fn landing(area: &crate::screen::WorkArea) -> (i32, i32) {
    let scale = area.scale.max(1.0);
    let w = W_LOGICAL * scale;
    let h = H_LOGICAL * scale;
    (
        MARGIN.mul_add(-scale, area.x + area.w - w).round() as i32,
        MARGIN.mul_add(-scale, area.y + area.h - h).round() as i32,
    )
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
            size: f32::from(size.unwrap_or(html::BASE_FONT_SIZE)),
        })
        .collect();
    popup.set_body_lines(slint::ModelRc::new(slint::VecModel::from(lines)));
}

#[cfg(test)]
mod tests {
    use super::get_or_try_init;

    #[test]
    fn get_or_try_init_reuses_existing_value() {
        let mut slot = None;
        let mut creates = 0;

        let (_, first_created) = get_or_try_init(&mut slot, || {
            creates += 1;
            Ok::<_, ()>("window")
        })
        .unwrap();
        let (value, second_created) = get_or_try_init(&mut slot, || {
            creates += 1;
            Ok::<_, ()>("replacement")
        })
        .unwrap();

        assert!(first_created);
        assert!(!second_created);
        assert_eq!(creates, 1);
        assert_eq!(*value, "window");
    }
}
