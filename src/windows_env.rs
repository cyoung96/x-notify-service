//! Windows 平台环境集成:GUI 子系统的控制台挂接与用户 PATH 管理

// CLI 终端输出语义
#![allow(clippy::print_stderr)]
/// GUI 子系统二进制无控制台:从 cmd/PowerShell 启动 CLI 子命令时把输出接回
/// 调用方终端(AttachConsole 父进程);双击/自启动/安装器等无控制台场景
/// 挂接失败,保持静默,不影响服务运行。
#[cfg(windows)]
// 取屏 FFI 同款豁免:仅此一处 Win32 控制台挂接
#[allow(unsafe_code, clippy::multiple_unsafe_ops_per_block)]
pub fn attach_parent_console() {
    use windows_sys::Win32::Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_WRITE, OPEN_EXISTING};
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
    };

    // SAFETY: 常量入参;句柄仅转交标准设备表,失败即原样返回静默运行
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        let name: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        let con = CreateFileW(
            name.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if con != INVALID_HANDLE_VALUE {
            SetStdHandle(STD_OUTPUT_HANDLE, con);
            SetStdHandle(STD_ERROR_HANDLE, con);
        }
    }
}

#[cfg(not(windows))]
pub const fn attach_parent_console() {}

/// 弹窗不占任务栏条目:按窗口标题(与 Linux 同口径)找到 HWND,加 WS_EX_TOOLWINDOW。
/// 须在 show() 之前调用,否则任务栏按钮已创建。
/// (通知类窗口不应出现在任务栏;Slint/winit 不透出 skip-taskbar 属性)
#[cfg(windows)]
// 取屏 FFI 同款豁免
#[allow(unsafe_code, clippy::multiple_unsafe_ops_per_block)]
pub fn skip_taskbar() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_TOOLWINDOW,
    };
    let title: Vec<u16> = "x-notify-service\0".encode_utf16().collect();
    // SAFETY: 常量入参;仅查标题匹配的窗口句柄,不做其他操作
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if hwnd.is_null() {
        return;
    }
    // SAFETY: HWND 来自 FindWindow(有效窗口);仅读写扩展样式位
    let ex_style = WS_EX_TOOLWINDOW as isize;
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | ex_style);
    }
}

#[cfg(windows)]
// Win32 注册表/消息广播 FFI
#[allow(unsafe_code, clippy::multiple_unsafe_ops_per_block)]
pub fn set_user_path(add: bool) {
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
    let (words, _) = raw.bytes.as_chunks::<2>();
    let words: Vec<u16> = words
        .iter()
        .map(|b| {
            let [lo, hi] = *b;
            (u16::from(hi) << 8) | u16::from(lo)
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
