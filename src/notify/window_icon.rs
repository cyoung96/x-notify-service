//! Linux 窗口图标直写:slint 的 icon 属性只能设单张,
//! 这里按 EWMH 偏好序补齐多档,任务栏取最近尺寸免缩放。

#[cfg(target_os = "linux")]
/// 直写多档窗口图标:slint 只能设单张,这里按大→小补齐四档,
/// 任务栏取最近尺寸免缩放
#[cfg(target_os = "linux")]
pub(super) fn set() {
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::ConnectionExt as _;

    const ICONS: [&[u8]; 4] = [
        include_bytes!("../../assets/icons/hicolor/x-notify-service-128.png"),
        include_bytes!("../../assets/icons/hicolor/x-notify-service-48.png"),
        include_bytes!("../../assets/icons/hicolor/x-notify-service-32.png"),
        include_bytes!("../../assets/icons/hicolor/x-notify-service-16.png"),
    ];
    let mut data: Vec<u32> = Vec::new();
    for bytes in ICONS {
        let Some(img) = decode_rgba(bytes) else {
            log::warn!("内嵌图标解码失败,跳过一档");
            continue;
        };
        data.push(img.0);
        data.push(img.1);
        data.extend(img.2);
    }
    let Ok((conn, screen_num)) = x11rb::connect(None) else {
        log::debug!("无 X 连接,跳过多档图标");
        return;
    };
    let root = conn.setup().roots[screen_num].root;
    if let Some(wid) = find_window(&conn, root) {
        write_icons(&conn, wid, &data);
    }
}

/// 定位本服务窗口:EWMH 客户端清单优先;精简 WM 不维护清单时递归全树兜底
/// (WM 会把客户端收进框架窗,须深入子树查找)
fn find_window(conn: &x11rb::rust_connection::RustConnection, root: u32) -> Option<u32> {
    let list_atom = conn
        .intern_atom(false, b"_NET_CLIENT_LIST")
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|a| a.atom)?;
    // 清单路径
    if let Some(reply) = conn
        .get_property(
            false,
            root,
            list_atom,
            x11rb::protocol::xproto::AtomEnum::WINDOW,
            0,
            1024,
        )
        .ok()
        .and_then(|c| c.reply().ok())
    {
        // X11 线序为小端
        let (words, _) = reply.value.as_chunks::<4>();
        let ids = words.iter().map(|c| u32::from_le_bytes(*c));
        for wid in ids {
            if window_matches(conn, wid) {
                return Some(wid);
            }
        }
    }
    dfs_find(conn, root, 0)
}

fn dfs_find(conn: &x11rb::rust_connection::RustConnection, wid: u32, depth: u8) -> Option<u32> {
    if depth > 8 {
        return None;
    }
    if window_matches(conn, wid) {
        return Some(wid);
    }
    let children = conn
        .query_tree(wid)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| r.children);
    for child in children.into_iter().flatten() {
        if let Some(hit) = dfs_find(conn, child, depth + 1) {
            return Some(hit);
        }
    }
    None
}

/// 写入多档图标属性
fn write_icons(conn: &x11rb::rust_connection::RustConnection, wid: u32, data: &[u32]) {
    use x11rb::protocol::xproto::ConnectionExt as _;
    let Some(atom) = conn
        .intern_atom(false, b"_NET_WM_ICON")
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|a| a.atom)
    else {
        return;
    };
    let bytes: Vec<u8> = data.iter().copied().flat_map(u32::to_ne_bytes).collect();
    let Ok(len) = u32::try_from(bytes.len()) else {
        return;
    };
    let _ = conn.change_property(
        x11rb::protocol::xproto::PropMode::REPLACE,
        wid,
        atom,
        x11rb::protocol::xproto::AtomEnum::CARDINAL,
        32,
        len,
        &bytes,
    );
    let _ = conn.flush();
    log::debug!("已写入多档窗口图标");
}

/// 按窗口名识别本服务窗口:winit 设置的名称属性类型不定,两种都查
#[cfg(target_os = "linux")]
fn window_matches(conn: &x11rb::rust_connection::RustConnection, wid: u32) -> bool {
    use x11rb::protocol::xproto::ConnectionExt as _;

    let target = b"x-notify-service";
    let prop_is = |prop: u32, ty: u32| {
        conn.get_property(false, wid, prop, ty, 0, 64)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|r| r.value == target)
    };
    let atom = |name: &[u8]| {
        conn.intern_atom(false, name)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|a| a.atom)
    };
    prop_is(
        x11rb::protocol::xproto::AtomEnum::WM_NAME.into(),
        x11rb::protocol::xproto::AtomEnum::STRING.into(),
    ) || match (atom(b"_NET_WM_NAME"), atom(b"UTF8_STRING")) {
        (Some(name), Some(utf8)) => prop_is(name, utf8),
        _ => false,
    }
}
