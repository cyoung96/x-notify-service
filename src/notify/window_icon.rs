//! 窗口多档图标:构建期生成 X11 线序负载(纯数据,跨平台编译与测试),
//! X11 直写仅 Linux 运行时;slint 的 icon 属性只能设单张,这里补齐四档供任务栏取最近尺寸。

include!(concat!(env!("OUT_DIR"), "/window_icons.rs"));

/// format=32 时 data_length 是 32 位元素个数(非字节数);
/// 传字节长会触发 x11rb 客户端断言 panic(UOS 闪退根因,回归测试钉死)
pub(super) fn property_units(blob: &[u8]) -> u32 {
    // 字节长转 u32 元素数:右移两位即 /4
    u32::try_from(blob.len() >> 2).unwrap_or(u32::MAX)
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{ICON_BLOB, property_units};
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::ConnectionExt as _;

    pub(super) fn set() {
        let Ok((conn, screen_num)) = x11rb::connect(None) else {
            log::debug!("无 X 连接,跳过多档图标");
            return;
        };
        let root = conn.setup().roots[screen_num].root;
        if let Some(wid) = find_window(&conn, root) {
            write_blob(&conn, wid, ICON_BLOB);
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

    /// 写入图标负载:blob 已是构建期产出的线序字节,直接落属性
    fn write_blob(conn: &x11rb::rust_connection::RustConnection, wid: u32, blob: &[u8]) {
        let Some(atom) = conn
            .intern_atom(false, b"_NET_WM_ICON")
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|a| a.atom)
        else {
            return;
        };
        let _ = conn.change_property(
            x11rb::protocol::xproto::PropMode::REPLACE,
            wid,
            atom,
            x11rb::protocol::xproto::AtomEnum::CARDINAL,
            32,
            property_units(blob),
            blob,
        );
        let _ = conn.flush();
        log::debug!("已写入多档窗口图标");
    }
}

#[cfg(target_os = "linux")]
pub(super) fn set() {
    // 图标是装饰性功能:任何 panic 拦截降级为 error 日志,
    // 绝不允许它杀死事件循环线程(UOS 闪退事故教训)
    if std::panic::catch_unwind(imp::set).is_err() {
        log::error!("窗口图标写入失败(panic 已拦截),通知功能不受影响");
    }
}

#[cfg(test)]
mod tests {
    use super::{ICON_BLOB, property_units};

    /// 闪退根因回归:字节数必须换算为 32 位元素数
    #[test]
    fn property_length_counts_elements() {
        assert_eq!(property_units(&[0u8; 16]), 4);
        let big = vec![0u8; 79_904];
        assert_eq!(property_units(&big), 19_976);
    }

    /// 构建期生成的负载可整除且恰含四档图标(128/48/32/16)
    #[test]
    fn icon_blob_structure_valid() {
        assert_eq!(ICON_BLOB.len() & 3, 0, "负载须为 u32 流");
        let le = |o: usize| u32::from_le_bytes(ICON_BLOB[o..o + 4].try_into().unwrap());
        let mut off = 0;
        let mut sizes = Vec::new();
        while off < ICON_BLOB.len() {
            let (w, h) = (le(off) as usize, le(off + 4) as usize);
            sizes.push(w);
            assert_eq!(w, h, "图标须为正方形");
            off += 8 + w * h * 4;
        }
        assert_eq!(off, ICON_BLOB.len(), "负载无尾部残渣");
        assert_eq!(sizes, vec![128, 48, 32, 16], "四档按偏好序");
    }
}
