use crate::{autostart, protocol};

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
    start_service_detached();
    println!("安装完成,服务已在后台启动");
}

/// uninstall 子命令:清理全部注册项(服务进程不在此处结束,重启或手动退出后不再自启)
pub fn uninstall() {
    match autostart::disable() {
        Ok(()) => println!("已移除开机自启动"),
        Err(e) => eprintln!("移除自启动失败: {e}"),
    }
    match protocol::unregister() {
        Ok(()) => println!("已注销 {}:// 协议", protocol::SCHEME),
        Err(e) => eprintln!("注销 {}:// 协议失败: {e}", protocol::SCHEME),
    }
    println!("卸载完成(正在运行的实例请手动退出或重启)");
}

/// 分离启动服务进程(重新拉起自身,无参数 = 服务模式)
fn start_service_detached() {
    let Ok(exe) = std::env::current_exe() else {
        log::warn!("无法定位自身路径,跳过后台启动");
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP:无控制台窗口、独立进程组
        cmd.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    match cmd.spawn() {
        Ok(child) => {
            log::info!("服务进程已分离启动(pid {})", child.id());
            // 丢弃句柄,让子进程完全独立
            drop(child);
        }
        Err(e) => log::warn!("后台启动服务失败: {e}(可手动运行 x-notify-service)"),
    }
}
