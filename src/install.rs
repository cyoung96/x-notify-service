// install/uninstall 为 CLI 用户交互命令,输出直连终端(此时可能无日志文件)
#![allow(clippy::print_stdout, clippy::print_stderr)]

use crate::{autostart, ctl, protocol};

#[cfg(windows)]
// Win32 注册表/消息广播 FFI
#[allow(unsafe_code, clippy::multiple_unsafe_ops_per_block)]
fn set_user_path(add: bool) {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::{RegKey, RegValue};

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(dir) = exe.parent() else { return };
    let dir_text = dir.to_string_lossy().to_string();

    let env = match RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
    {
        Ok(k) => k,
        Err(e) => {
            eprintln!("配置用户 PATH 失败: {e}");
            return;
        }
    };
    let raw = env.get_raw_value("Path").unwrap_or(RegValue {
        bytes: Vec::new(),
        vtype: winreg::enums::REG_EXPAND_SZ,
    });
    // 注册表 Path 值为 UTF-16LE:位拼解码(低字节在前)
    let words: Vec<u16> = raw
        .bytes
        .chunks_exact(2)
        .map(|b| match b {
            [lo, hi] => (u16::from(*hi) << 8) | u16::from(*lo),
            _ => 0,
        })
        .collect();
    let cur = String::from_utf16_lossy(&words);
    let mut entries: Vec<String> = cur
        .split(';')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let exists = entries.iter().any(|e| e.eq_ignore_ascii_case(&dir_text));
    let changed = match (add, exists) {
        (true, false) => {
            entries.push(dir_text);
            true
        }
        (false, true) => {
            entries.retain(|e| !e.eq_ignore_ascii_case(&dir_text));
            true
        }
        _ => false,
    };
    if !changed {
        return;
    }
    let joined = format!("{}\0", entries.join(";"));
    let bytes: Vec<u8> = joined.encode_utf16().flat_map(u16::to_le_bytes).collect();
    if let Err(e) = env.set_raw_value(
        "Path",
        &RegValue {
            bytes,
            vtype: raw.vtype,
        },
    ) {
        eprintln!("写入用户 PATH 失败: {e}");
        return;
    }
    // 环境变更广播:新开终端即可命中,已开终端需重开
    let param: Vec<u16> = "Environment\0".encode_utf16().collect();
    // SAFETY: 广播常量消息;指针指向本栈上有效的 UTF-16 参数串
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SendMessageTimeoutW(
            windows_sys::Win32::UI::WindowsAndMessaging::HWND_BROADCAST,
            windows_sys::Win32::UI::WindowsAndMessaging::WM_SETTINGCHANGE,
            0,
            param.as_ptr() as isize,
            windows_sys::Win32::UI::WindowsAndMessaging::SMTO_ABORTIFHUNG,
            3000,
            std::ptr::null_mut(),
        );
    };
}

/// install 子命令:注册自启动 + x-notify:// 协议(失败仅告警,不阻塞),
/// 随后分离启动服务进程并立即返回,供安装器/脚本调用不阻塞。
pub fn install() {
    match autostart::enable() {
        Ok(()) => log::info!("已注册开机自启动"),
        Err(e) => log::warn!("注册开机自启动失败: {e}"),
    }
    match protocol::register() {
        Ok(()) => log::info!("已注册 {}:// 协议", protocol::SCHEME),
        Err(e) => log::warn!("注册 {}:// 协议失败: {e}", protocol::SCHEME),
    }
    #[cfg(windows)]
    set_user_path(true);
    ctl::start_detached();
    println!("安装完成,服务已在后台启动");
}

/// uninstall 子命令:先停止运行中的服务,再清理全部注册项,
/// 避免留下「还在跑但不再自启」的半卸载状态
pub fn uninstall() {
    #[cfg(windows)]
    set_user_path(false);
    ctl::stop();
    match autostart::disable() {
        Ok(()) => println!("已移除开机自启动"),
        Err(e) => eprintln!("移除自启动失败: {e}"),
    }
    match protocol::unregister() {
        Ok(()) => println!("已注销 {}:// 协议", protocol::SCHEME),
        Err(e) => eprintln!("注销 {}:// 协议失败: {e}", protocol::SCHEME),
    }
    println!("卸载完成(二进制与日志保留,可手动删除)");
}
