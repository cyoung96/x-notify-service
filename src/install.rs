// install/uninstall 为 CLI 用户交互命令,输出直连终端(此时可能无日志文件)
#![allow(clippy::print_stdout, clippy::print_stderr)]

use crate::{autostart, ctl, protocol};

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
    crate::windows_env::set_user_path(true);
    ctl::start_detached();
    println!("安装完成,服务已在后台启动");
}

/// uninstall 子命令:先停止运行中的服务,再清理全部注册项,
/// 避免留下「还在跑但不再自启」的半卸载状态
pub fn uninstall() {
    #[cfg(windows)]
    crate::windows_env::set_user_path(false);
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
