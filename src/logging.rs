use std::io::Write as _;

use flexi_logger::{
    Age, Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming, WriteMode,
};
use flexi_logger::LoggerHandle;

use crate::config::Config;

/// 初始化日志:按天滚动、保留 7 天,warn 及以上同步镜像到 stderr。
/// 返回的 LoggerHandle 需在 main 存活期间保持持有。
pub fn init(cfg: &Config) -> Option<LoggerHandle> {
    if let Err(e) = std::fs::create_dir_all(&cfg.log_dir) {
        eprintln!("警告: 无法创建日志目录 {}: {e}", cfg.log_dir.display());
        return None;
    }
    let spec = FileSpec::default()
        .directory(&cfg.log_dir)
        .basename(crate::config::APP_DIR_NAME)
        .suppress_timestamp();

    let base = Logger::try_with_env_or_str(&cfg.log_level)
        .or_else(|_| Logger::try_with_str("info"))
        .expect("日志级别解析失败");
    let logger = base
        .log_to_file(spec)
        .rotate(
            Criterion::Age(Age::Day),
            Naming::Timestamps,
            Cleanup::KeepLogFiles(7),
        )
        .duplicate_to_stderr(Duplicate::Warn)
        .write_mode(WriteMode::BufferAndFlush)
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
