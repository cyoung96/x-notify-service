//! Linux 窗口图标直写:slint 的 icon 属性只能设单张,
//! 这里按 EWMH 偏好序补齐多档,任务栏取最近尺寸免缩放。

#[cfg(target_os = "linux")]
mod imp {
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::ConnectionExt as _;

    pub(super) fn set() {
        const ICONS: [&[u8]; 4] = [
            include_bytes!("../../assets/icons/hicolor/x-notify-service-128.png"),
            include_bytes!("../../assets/icons/hicolor/x-notify-service-48.png"),
            include_bytes!("../../assets/icons/hicolor/x-notify-service-32.png"),
            include_bytes!("../../assets/icons/hicolor/x-notify-service-16.png"),
        ];
        let mut data: Vec<u32> = Vec::new();
        for bytes in ICONS {
            match decode_rgba(bytes) {
                Some(img) => {
                    data.push(img.0);
                    data.push(img.1);
                    data.extend(img.2);
                }
                None => log::warn!("内嵌图标解码失败,跳过一档"),
            }
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
            for wid in words.iter().map(|c| u32::from_le_bytes(*c)) {
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

    /// 按窗口名识别本服务窗口:winit 设置的名称属性类型不定,两种都查
    fn window_matches(conn: &x11rb::rust_connection::RustConnection, wid: u32) -> bool {
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

    /// 写入多档图标属性
    fn write_icons(conn: &x11rb::rust_connection::RustConnection, wid: u32, data: &[u32]) {
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

    /// 解码内嵌 PNG 为宽高与 ARGB 像素
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
}

#[cfg(target_os = "linux")]
pub(super) fn set() {
    imp::set();
}
