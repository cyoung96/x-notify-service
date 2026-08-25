/// 屏幕主显示器「工作区」(排除任务栏/dock/菜单栏),原点左上。
/// 所有坐标已统一换算为物理像素;scale 仅供逻辑尺寸(弹窗宽高)换算,
/// Windows/Linux 为 1.0,macOS 为 backingScaleFactor。
#[derive(Debug, Clone, Copy)]
pub struct WorkArea {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub scale: f64,
}

#[cfg(windows)]
pub fn work_area() -> Option<WorkArea> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETWORKAREA, SystemParametersInfoW};

    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let ok = unsafe {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut RECT as *mut core::ffi::c_void, 0)
    };
    if ok == 0 {
        return None;
    }
    Some(WorkArea {
        x: rect.left as f64,
        y: rect.top as f64,
        w: (rect.right - rect.left) as f64,
        h: (rect.bottom - rect.top) as f64,
        scale: 1.0,
    })
}

#[cfg(target_os = "macos")]
pub fn work_area() -> Option<WorkArea> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;

    // 只能在主线程(GUI 事件循环线程)查询;坐标原点在左下,换算为左上原点物理像素
    let mtm = MainThreadMarker::new()?;
    let screen = NSScreen::mainScreen(mtm)?;
    let full = screen.frame();
    let vis = screen.visibleFrame();
    let scale = screen.backingScaleFactor();
    let y_top_pts = (full.origin.y + full.size.height) - (vis.origin.y + vis.size.height);
    Some(WorkArea {
        x: vis.origin.x * scale,
        y: y_top_pts * scale,
        w: vis.size.width * scale,
        h: vis.size.height * scale,
        scale,
    })
}

#[cfg(target_os = "linux")]
pub fn work_area() -> Option<WorkArea> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::ConnectionExt;

    // Wayland(含 XWayland:DISPLAY/WAYLAND_DISPLAY 并存):协议禁止客户端自定位,
    // 弹窗会被合成器随手摆放(常为左上角),返回 None 走系统通知兜底
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        log::info!("Wayland/XWayland 会话,弹窗自定位不可用,通知走系统通道");
        return None;
    }
    let (conn, screen_num) = x11rb::connect(None).ok()?;
    let screen = conn.setup().roots.get(screen_num)?;
    let full = WorkArea {
        x: 0.0,
        y: 0.0,
        w: screen.width_in_pixels as f64,
        h: screen.height_in_pixels as f64,
        scale: 1.0,
    };
    // 尝试读 _NET_WORKAREA(第一个桌面的 x/y/w/h)
    let wa = (|| -> Option<(f64, f64, f64, f64)> {
        let atom = conn.intern_atom(false, b"_NET_WORKAREA").ok()?.reply().ok()?;
        let reply = conn
            .get_property(
                false,
                screen.root,
                atom.atom,
                x11rb::protocol::xproto::AtomEnum::CARDINAL,
                0,
                4,
            )
            .ok()?
            .reply()
            .ok()?;
        let v = &reply.value;
        if v.len() < 16 {
            return None;
        }
        let u = |i: usize| {
            u32::from_ne_bytes([v[i], v[i + 1], v[i + 2], v[i + 3]]) as f64
        };
        Some((u(0), u(1), u(2), u(3)))
    })();
    match wa {
        Some((x, y, w, h)) => Some(WorkArea { x, y, w, h, scale: 1.0 }),
        None => {
            log::info!("WM 未提供 _NET_WORKAREA(精简桌面?),退回全屏尺寸定位");
            Some(full)
        }
    }
}
