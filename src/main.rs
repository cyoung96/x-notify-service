// Windows 发布形态为 GUI 子系统:服务常驻不弹控制台窗口,输出走日志文件
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod autostart;
mod config;
mod html;
mod install;
mod logging;
mod notify;
mod protocol;
mod screen;
mod server;
mod single;

use clap::Parser;

slint::include_modules!();

fn main() {
    let cli = config::Cli::parse();
    let cfg = config::resolve(&cli);
    let _logger = logging::init(&cfg);

    match cli.cmd {
        // 无参数:运行服务(即常驻后台进程本体)
        None => serve(cfg),
        Some(config::Command::Install) => {
            // 注册 + 分离启动服务后立即退出,供安装器/脚本调用不阻塞
            install::install();
        }
        Some(config::Command::Uninstall) => {
            install::uninstall();
        }
    }
}

/// 服务主流程:单实例 → 绑端口 → HTTP 线程 → GUI(或降级挂起)
fn serve(cfg: config::Config) -> ! {
    log::info!(
        "x-notify-service {} 启动(默认端口 {},日志目录 {})",
        api::VERSION,
        cfg.port,
        cfg.log_dir.display()
    );

    if !single::acquire_lock() {
        log::info!("已有实例在运行,本进程静默退出");
        std::process::exit(0);
    }

    let port = server::start(cfg.clone());
    single::write_port_file(port);
    log::info!("服务已就绪: http://127.0.0.1:{port}");

    let gui_ok = !cfg.no_popup && notify::popup::gui_probe();
    if !gui_ok {
        if cfg.no_popup {
            log::info!("--no-popup:通知全部走系统通知");
        } else {
            log::warn!("弹窗不可用,通知将走系统通知兜底");
        }
        notify::POPUP_AVAILABLE.store(false, std::sync::atomic::Ordering::Relaxed);
        // 无 GUI 模式:主线程挂起,HTTP 工作线程继续服务
        loop {
            std::thread::park();
        }
    }

    notify::POPUP_AVAILABLE.store(true, std::sync::atomic::Ordering::Relaxed);
    // until_quit:弹窗 hide 不允许结束事件循环(最后一个窗口关闭默认会退出循环)
    if let Err(e) = slint::run_event_loop_until_quit() {
        log::error!("事件循环异常退出: {e}");
    }
    std::process::exit(0);
}
