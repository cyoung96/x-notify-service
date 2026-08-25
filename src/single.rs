use std::fs::File;
use std::path::PathBuf;

/// 用户级数据目录(日志 port 文件等)
pub fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .map(|d| d.join(crate::config::APP_DIR_NAME))
        .unwrap_or_else(|| std::env::temp_dir().join(crate::config::APP_DIR_NAME))
}

/// 单实例锁文件位置:Linux 优先 XDG_RUNTIME_DIR,其余平台放数据目录
fn lock_path() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        if let Some(rt) = std::env::var_os("XDG_RUNTIME_DIR") {
            return PathBuf::from(rt).join(format!("{}.lock", crate::config::APP_DIR_NAME));
        }
    }
    data_dir().join("instance.lock")
}

/// 尝试获取单实例锁;false 表示已有实例在运行。
/// flock 语义:进程退出(含崩溃)自动释放,锁守卫被故意泄漏以持有到进程结束。
pub fn acquire_lock() -> bool {
    let path = lock_path();
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err() {
            return true; // 锁目录不可用时不阻止启动
        }
    let Ok(file) = File::create(&path) else {
        return true;
    };
    let lock: &'static mut fd_lock::RwLock<File> =
        Box::leak(Box::new(fd_lock::RwLock::new(file)));
    match lock.try_write() {
        Ok(guard) => {
            std::mem::forget(guard);
            log::debug!("单实例锁已获取: {}", path.display());
            true
        }
        Err(_) => false,
    }
}

/// 把实际端口 + PID 写入数据目录,便于本地工具/调试定位服务
pub fn write_port_file(port: u16) {
    let dir = data_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let content = format!("{{\"port\":{port},\"pid\":{}}}\n", std::process::id());
    let _ = std::fs::write(dir.join("port"), content);
}
