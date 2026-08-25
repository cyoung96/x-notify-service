use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub const DEFAULT_PORT: u16 = 17320;

#[derive(Debug, Parser)]
#[command(
    name = "x-notify-service",
    version,
    about = "跨平台消息通知服务:浏览器调用 → 右下角置顶弹窗(系统通知兜底)"
)]
pub struct Cli {
    /// 协议拉起(x-notify://)时由系统传入的 URL,忽略即可
    #[arg(hide = true)]
    pub url_arg: Option<String>,

    /// 监听端口(被占用时自动向后探测 10 个)
    #[arg(long)]
    pub port: Option<u16>,

    /// 配置文件路径(默认: 二进制同目录 > 平台用户配置目录)
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// 日志级别(trace/debug/info/warn/error)
    #[arg(long)]
    pub log_level: Option<String>,

    /// 日志目录
    #[arg(long)]
    pub log_dir: Option<PathBuf>,

    /// 禁用弹窗,强制走系统通知
    #[arg(long)]
    pub no_popup: bool,

    /// Windows 兜底通知的 AppId(默认 PowerShell)
    #[arg(long)]
    pub app_id: Option<String>,

    #[command(subcommand)]
    pub cmd: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// 注册开机自启动 + x-notify:// 协议,并启动服务
    Install,
    /// 清理全部注册项(自启动 + 协议)
    Uninstall,
}

/// 最终生效配置(CLI 参数 > 配置文件 > 默认值)
#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub log_level: String,
    pub log_dir: PathBuf,
    pub no_popup: bool,
    /// Windows 兜底通知的 AppId(仅 Windows 读取)
    #[cfg_attr(not(windows), allow(dead_code))]
    pub app_id: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct FileConfig {
    port: Option<u16>,
    log_level: Option<String>,
    log_dir: Option<PathBuf>,
    no_popup: Option<bool>,
    app_id: Option<String>,
}

/// 平台标准日志目录
pub fn default_log_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        // ~/Library/Logs
        dirs::home_dir()
            .map(|h| h.join("Library").join("Logs").join(APP_DIR_NAME))
            .unwrap_or_else(|| PathBuf::from(".").join("logs"))
    } else if cfg!(windows) {
        // %LOCALAPPDATA%
        dirs::data_local_dir()
            .map(|d| d.join(APP_DIR_NAME).join("logs"))
            .unwrap_or_else(|| PathBuf::from(".").join("logs"))
    } else {
        // XDG: ~/.local/state
        dirs::state_dir()
            .map(|d| d.join(APP_DIR_NAME).join("logs"))
            .unwrap_or_else(|| PathBuf::from(".").join("logs"))
    }
}

/// 平台用户配置目录
fn user_config_path() -> Option<PathBuf> {
    dirs::config_local_dir()
        .or_else(dirs::config_dir)
        .map(|d| d.join(APP_DIR_NAME).join("config.toml"))
}

pub const APP_DIR_NAME: &str = "x-notify-service";

pub fn resolve(cli: &Cli) -> Config {
    let file = load_file_config(cli.config.as_deref());
    Config {
        port: cli.port.or(file.port).unwrap_or(DEFAULT_PORT),
        log_level: cli
            .log_level
            .clone()
            .or(file.log_level)
            .unwrap_or_else(|| "info".into()),
        log_dir: cli
            .log_dir
            .clone()
            .or(file.log_dir)
            .unwrap_or_else(default_log_dir),
        no_popup: cli.no_popup || file.no_popup.unwrap_or(false),
        app_id: cli.app_id.clone().or(file.app_id),
    }
}

/// 配置文件查找顺序:--config > 二进制同目录(绿色版友好) > 平台用户配置目录
fn load_file_config(explicit: Option<&std::path::Path>) -> FileConfig {
    let candidates: Vec<PathBuf> = match explicit {
        Some(p) => vec![p.to_path_buf()],
        None => {
            let mut v = Vec::new();
            if let Ok(exe) = std::env::current_exe()
                && let Some(dir) = exe.parent() {
                    v.push(dir.join("config.toml"));
                }
            if let Some(p) = user_config_path() {
                v.push(p);
            }
            v
        }
    };
    for path in candidates {
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<FileConfig>(&text) {
                Ok(cfg) => {
                    log::debug!("已加载配置文件: {}", path.display());
                    return cfg;
                }
                Err(e) => {
                    eprintln!("警告: 配置文件 {} 解析失败: {e}", path.display());
                }
            },
            Err(_) => continue,
        }
    }
    FileConfig::default()
}
