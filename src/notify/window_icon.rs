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
    // 客户端窗口清单走 EWMH _NET_CLIENT_LIST:WM 会把窗口重挂进自己的框架,
    // root 直接子窗口是框架而非本窗,遍历树必落空
    let list_atom = conn
        .intern_atom(false, b"_NET_CLIENT_LIST")
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|a| a.atom);
    let Some(list_atom) = list_atom else { return };
    let clients = conn
        .get_property(
            false,
            root,
            list_atom,
            x11rb::protocol::xproto::AtomEnum::WINDOW,
            0,
            1024,
        )
        .ok()
        .and_then(|c| c.reply().ok());
    let Some(clients) = clients else { return };
    // X11 线序为小端
    let (words, _) = clients.value.as_chunks::<4>();
    let ids: Vec<u32> = words.iter().map(|c| u32::from_le_bytes(*c)).collect();
    for wid in ids {
        if window_matches(&conn, wid) {
            let atom = conn
                .intern_atom(false, b"_NET_WM_ICON")
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|a| a.atom);
            let Some(atom) = atom else { return };
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
            log::debug!("已写入多档窗口图标({} 档)", ICONS.len());
            return;
        }
    }
}

/// 解码内嵌 PNG 为宽高与 ARGB 像素
#[cfg(target_os = "linux")]
fn decode_rgba(bytes: &[u8]) -> Option<(u32, u32, Vec<u32>)> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (chunks, _) = buf.as_chunks::<4>();
    let argb = chunks
        .iter()
        .map(|px| {
            let [r, g, b, a] = *px;
            (u32::from(a) << 24) | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
        })
        .collect();
    Some((info.width.min(512), info.height.min(512), argb))
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
