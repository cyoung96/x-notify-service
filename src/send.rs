//! `notify` 子命令:本机发一条通知——安装前手测弹窗/兜底用。
//! 服务在运行时经其 HTTP 通道投递(与浏览器/SDK 同路径),CLI 立即返回;
//! 未运行时本进程弹窗(窗口需事件循环驻留,点击关闭后进程退出);
//! `-f` 恒走本机系统通知(立即返回);`close` 子命令关闭当前弹窗(幂等)。

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

    // 服务在运行:走 HTTP 通道(弹窗归服务持有,CLI 立即返回)
    if !fallback
        && let Some(rec) = crate::single::read_port_file()
        && matches!(
            crate::ctl::probe_port(rec.port),
            Some(crate::ctl::Probe::Ours { .. })
        )
    {
        deliver_via_service(rec.port, &title, &body_html);
        return;
    }

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
    println!("弹窗已显示(无运行中服务,本进程驻留至点击关闭)");
    if let Err(e) = slint::run_event_loop_until_quit() {
        eprintln!("事件循环异常: {e}");
        std::process::exit(1);
    }
}

/// 经运行中服务的 /notify 投递(与浏览器/SDK 完全同路径)
fn deliver_via_service(port: u16, title: &str, body_html: &str) {
    let payload = serde_json::json!({ "title": title, "body": body_html }).to_string();
    match crate::ctl::request(port, "POST", "/notify", &payload) {
        Some(resp) if resp.contains("\"ok\":true") => {
            let via = serde_json::from_str::<serde_json::Value>(&resp)
                .ok()
                .and_then(|v| v["via"].as_str().map(str::to_string))
                .unwrap_or_else(|| "?".into());
            println!("已投递(via={via},经运行中服务)");
        }
        Some(resp) => {
            eprintln!("服务拒绝投递: {resp}");
            std::process::exit(1);
        }
        None => {
            eprintln!("服务连接失败");
            std::process::exit(1);
        }
    }
}

/// close 子命令:服务未运行时幂等成功
pub fn close() {
    let Some(rec) = crate::single::read_port_file() else {
        println!("服务未在运行,无弹窗可关");
        return;
    };
    match crate::ctl::request(rec.port, "POST", "/close", "") {
        Some(body) if body.contains("\"ok\":true") => println!("已关闭当前弹窗"),
        Some(_) => println!("服务已应答但未确认关闭(详见服务日志)"),
        None => println!("服务未在运行,无弹窗可关"),
    }
}
