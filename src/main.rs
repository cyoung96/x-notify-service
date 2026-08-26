// Windows 发布形态为 GUI 子系统:服务常驻不弹控制台窗口,输出走日志文件
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod autostart;
mod config;
mod ctl;
mod html;
mod info;
mod install;
mod logging;
mod notify;
mod protocol;
mod screen;
mod send;
mod server;
mod single;

use clap::Parser as _;

// Slint 生成代码(OUT_DIR/popup.rs)是机器产物:保留 correctness 审查,
// 其余风格/限制组豁免(直接标在宏调用上的 allow 会被 rustc 忽略,必须包 mod);
// unsafe_code 放行因跨平台生成物(软件渲染)内含既定 unsafe
#[allow(
    unsafe_code,
    clippy::style,
    clippy::complexity,
    clippy::perf,
    clippy::pedantic,
    clippy::nursery,
    clippy::restriction,
    clippy::cargo
)]
mod slint_generated {
    slint::include_modules!();
}
pub use slint_generated::*;

/// GUI 子系统二进制无控制台:从 cmd/PowerShell 启动 CLI 子命令时把输出接回
/// 调用方终端(AttachConsole 父进程);双击/自启动/安装器等无控制台场景
/// 挂接失败,保持静默,不影响服务运行。
#[cfg(windows)]
// 取屏 FFI 同款豁免:仅此一处 Win32 控制台挂接
#[allow(unsafe_code, clippy::multiple_unsafe_ops_per_block)]
fn attach_parent_console() {
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
const fn attach_parent_console() {}

fn main() {
    attach_parent_console();
    let cli = config::Cli::parse();
    let cfg = config::resolve(&cli);
    // 文件日志仅服务进程需要;一次性 CLI 命令不建文件(避免空日志),输出走终端
    let serve_mode = match &cli.cmd {
        None => cli.url_arg.is_some(),
        Some(config::Command::Serve) => true,
        Some(_) => false,
    };
    let _logger = serve_mode.then(|| logging::init(&cfg));

    match cli.cmd {
        // 无参数:显示帮助;协议拉起(url 参数)时仍直接进入服务(单例语义)
        None => {
            if cli.url_arg.is_some() {
                serve(&cfg);
            } else {
                use clap::CommandFactory as _;
                let _ = config::Cli::command().print_help();
            }
        }
        Some(config::Command::Serve) => serve(&cfg),
        Some(config::Command::Install) => {
            // 注册 + 分离启动服务后立即退出,供安装器/脚本调用不阻塞
            install::install();
        }
        Some(config::Command::Uninstall) => {
            install::uninstall();
        }
        Some(config::Command::Info) => {
            info::run(&cfg);
        }
        Some(config::Command::Start) => ctl::start(),
        Some(config::Command::Stop) => ctl::stop(),
        Some(config::Command::Restart) => ctl::restart(),
        Some(config::Command::Notify {
            title,
            body,
            fallback,
        }) => {
            send::run(&cfg, title, body, fallback);
        }
        Some(config::Command::Close) => send::close(),
    }
}

/// 服务主流程:单实例 → GUI 探测(先于 HTTP,避免启动早期请求降级)→ 绑端口 → 事件循环
fn serve(cfg: &config::Config) {
    log::info!(
        "x-notify-service {} 启动(默认端口 {},日志目录 {})",
        api::VERSION,
        cfg.port,
        cfg.log_dir.display()
    );

    if !single::acquire_lock() {
        log::info!("已有实例在运行,本进程静默退出");
        return;
    }

    // 先探测 GUI 并落定 POPUP_AVAILABLE,再开 HTTP:杜绝启动早期请求撞上降级窗口
    let gui_ok = !cfg.no_popup && notify::popup::gui_probe();
    notify::POPUP_AVAILABLE.store(gui_ok, std::sync::atomic::Ordering::Relaxed);
    if !gui_ok {
        if cfg.no_popup {
            log::info!("--no-popup:通知全部走系统通知");
        } else {
            log::warn!("弹窗不可用,通知将走系统通知兜底");
        }
    }

    let port = server::start(cfg.clone());
    single::write_port_file(port);
    log::info!("服务已就绪: http://127.0.0.1:{port}");

    if !gui_ok {
        // 无 GUI 模式:主线程挂起,HTTP 工作线程继续服务
        #[allow(clippy::infinite_loop)]
        loop {
            std::thread::park();
        }
    }

    // until_quit:弹窗 hide 不允许结束事件循环(最后一个窗口关闭默认会退出循环)
    if let Err(e) = slint::run_event_loop_until_quit() {
        log::error!("事件循环异常退出: {e}");
    }
}
