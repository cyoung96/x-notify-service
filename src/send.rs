//! `notify` 子命令:本机直接发一条通知,不经 HTTP——安装前手测弹窗/兜底用。
//! `-t` 标题必填,`-b` 正文可选(走 HTML 子集管线);默认弹窗(点击关闭后进程退出),
//! `-f` 强制走系统通知兜底。`close` 子命令:关闭当前弹窗(幂等)。

// CLI 子命令:进程退出码即结果语义,打印直连终端
#![allow(clippy::print_stdout, clippy::print_stderr, clippy::exit)]

use crate::config::Config;
use crate::notify;
use crate::notify::Presenter as _;

pub fn run(cfg: &Config, title: String, body: Option<String>, fallback: bool) {
    let req = crate::api::NotifyRequest { title, body };
    if let Err(e) = req.validate() {
        eprintln!("通知内容不合法: {e}");
        std::process::exit(2);
    }
    let title = req.title.trim().to_string();
    let body_html = req.body.unwrap_or_default();

    let gui_ok = !fallback
        && !cfg.no_popup
        && notify::popup::gui_probe()
        && crate::screen::work_area().is_some();
    if !gui_ok {
        let presenter = notify::fallback::SystemPresenter::new(cfg);
        if presenter.present(&title, &body_html) {
            println!("已发送系统通知(via=system)");
        } else {
            eprintln!("系统通知发送失败(详见日志)");
            std::process::exit(1);
        }
        return;
    }

    let posted =
        slint::invoke_from_event_loop(move || notify::popup::spawn(&title, &body_html, true));
    if posted.is_err() {
        eprintln!("弹窗投递失败");
        std::process::exit(1);
    }
    println!("弹窗已显示,点击关闭后退出(via=popup)");
    if let Err(e) = slint::run_event_loop_until_quit() {
        eprintln!("事件循环异常: {e}");
        std::process::exit(1);
    }
}

/// close 子命令:服务未运行时幂等成功
pub fn close() {
    let Some(rec) = crate::single::read_port_file() else {
        println!("服务未在运行,无弹窗可关");
        return;
    };
    match crate::ctl::request(rec.port, "POST", "/close") {
        Some(body) if body.contains("\"ok\":true") => println!("已关闭当前弹窗"),
        Some(_) => println!("服务已应答但未确认关闭(详见服务日志)"),
        None => println!("服务未在运行,无弹窗可关"),
    }
}
