// 日志尚未初始化时的兜底告警直写 stderr
#![allow(clippy::print_stderr)]

use std::io::Write as _;

use flexi_logger::LoggerHandle;
use flexi_logger::{Age, Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming, WriteMode};

use crate::config::Config;

/// 初始化日志(仅 serve 模式调用):按天滚动、保留 3 天(历史 3 + 当前 1,最多 4 个文件),
/// warn 及以上镜像 stderr;轮换/清理由输出线程承担。
/// 返回的 `LoggerHandle` 需在 main 存活期间保持持有(丢弃即停日志)。
pub fn init(cfg: &Config) -> Option<LoggerHandle> {
    if let Err(e) = std::fs::create_dir_all(&cfg.log_dir) {
        eprintln!("警告: 无法创建日志目录 {}: {e}", cfg.log_dir.display());
        return None;
    }
    let spec = FileSpec::default()
        .directory(&cfg.log_dir)
        .basename(crate::config::APP_DIR_NAME)
        .suppress_timestamp();

    // 兜底 "info" 是字面合法级别,此分支实际不可达;万一失败则降级为无日志运行
    let base = match Logger::try_with_env_or_str(&cfg.log_level)
        .or_else(|_| Logger::try_with_str("info"))
    {
        Ok(b) => b,
        Err(e) => {
            eprintln!("警告: 日志级别解析失败: {e}");
            return None;
        }
    };
    let logger = base
        .log_to_file(spec)
        .rotate(
            Criterion::Age(Age::Day),
            Naming::Timestamps,
            Cleanup::KeepLogFiles(3),
        )
        .duplicate_to_stderr(Duplicate::Warn)
        // 异步写:调用线程仅入队,输出线程持续落盘(流式)并每 300ms 刷盘
        .write_mode(WriteMode::AsyncWith {
            pool_capa: 64,
            message_capa: 256,
            flush_interval: std::time::Duration::from_millis(300),
        })
        .format_for_files(flexi_logger::opt_format);

    match logger.start() {
        Ok(handle) => {
            let _ = std::io::stdout().flush();
            Some(handle)
        }
        Err(e) => {
            eprintln!("警告: 日志初始化失败: {e}");
            None
        }
    }
}
