use std::io::Read;
use std::sync::Arc;

use tiny_http::{Header, Method, Request, Response, Server};

use crate::api::{self, ErrorResponse, HealthResponse, NotifyResponse};
use crate::config::Config;
use crate::notify;

const MAX_BODY: usize = 64 * 1024;
/// 单 HTTP 工作线程:进程模型保持最简(主线程 GUI 事件循环 + 1 个 HTTP 线程),
/// 本地单用户场景串行处理足够
const WORKERS: usize = 1;

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).expect("非法响应头")
}

fn cors_headers() -> Vec<Header> {
    vec![
        header("Access-Control-Allow-Origin", "*"),
        header("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
        header("Access-Control-Allow-Headers", "Content-Type"),
        // 新版 Chrome Local Network Access 预检需要;Chrome 87 忽略之
        header("Access-Control-Allow-Private-Network", "true"),
        header("Access-Control-Max-Age", "86400"),
    ]
}

fn send_json(req: Request, status: u16, body: String) {
    let mut response = Response::from_string(body)
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json; charset=utf-8"));
    for h in cors_headers() {
        response = response.with_header(h);
    }
    let _ = req.respond(response);
}

fn send_error(req: Request, err: &api::NotifyError) {
    let body = serde_json::to_string(&ErrorResponse { ok: false, error: err.to_string() })
        .unwrap_or_else(|_| r#"{"ok":false,"error":"internal"}"#.into());
    send_json(req, err.status(), body);
}

/// 绑定端口并启动 HTTP 服务:默认端口被占依次向后探测 10 个;
/// 全部被占则绑定随机端口(此时浏览器无法发现,仅系统通知可用)。
/// 返回实际端口。
pub fn start(cfg: Config) -> u16 {
    let mut server = None;
    let mut used_port = 0u16;
    for port in cfg.port..cfg.port.saturating_add(10) {
        match Server::http(("127.0.0.1", port)) {
            Ok(s) => {
                server = Some(s);
                used_port = port;
                break;
            }
            Err(_) => continue,
        }
    }
    let server = match server {
        Some(s) => s,
        None => {
            // 端口 0 让 OS 分配:先经 TcpListener 拿到可用端口再交给 tiny_http
            let free = std::net::TcpListener::bind("127.0.0.1:0")
                .and_then(|l| l.local_addr())
                .map_err(|e| e.to_string())
                .and_then(|a| Server::http(("127.0.0.1", a.port())).map_err(|e| e.to_string()))
                .expect("绑定随机端口失败");
            if let Some(addr) = free.server_addr().to_ip() {
                used_port = addr.port();
            }
            log::warn!(
                "端口 {}~{} 全部被占用,已绑定随机端口 {used_port},浏览器将无法自动发现服务",
                cfg.port,
                cfg.port + 9
            );
            free
        }
    };
    if used_port != cfg.port {
        log::warn!("默认端口 {} 被占用,实际监听 {used_port}", cfg.port);
    }

    let server = Arc::new(server);
    // 运行期共享状态:/health 报告实际监听端口(可能与配置端口不同)
    let state = Arc::new(Runtime { cfg, port: used_port });
    for i in 0..WORKERS {
        let server = Arc::clone(&server);
        let state = Arc::clone(&state);
        std::thread::Builder::new()
            .name(format!("http-{i}"))
            .spawn(move || loop {
                match server.recv() {
                    Ok(req) => handle(&state, req),
                    Err(e) => {
                        log::error!("接收请求失败: {e}");
                        return;
                    }
                }
            })
            .expect("启动 HTTP 工作线程失败");
    }
    used_port
}

/// HTTP 工作线程共享的运行期状态
struct Runtime {
    cfg: Config,
    port: u16,
}

fn handle(rt: &Runtime, req: Request) {
    let Runtime { cfg, port } = rt;
    let method = req.method().clone();
    let path = req.url().split('?').next().unwrap_or("/").to_string();

    if method == Method::Options {
        let mut response = Response::empty(204);
        for h in cors_headers() {
            response = response.with_header(h);
        }
        let _ = req.respond(response);
        return;
    }

    match (method, path.as_str()) {
        (Method::Get, "/health") => {
            let body = HealthResponse {
                app: api::APP_ID,
                version: api::VERSION,
                port: *port,
            };
            let _ = serde_json::to_string(&body).map(|b| send_json(req, 200, b));
        }
        (Method::Post, "/notify") => handle_notify(cfg, req),
        (Method::Post, "/close") => {
            // 显式关闭当前弹窗(幂等);经事件循环投递到 GUI 线程
            if let Err(e) = slint::invoke_from_event_loop(notify::popup::close_current) {
                log::warn!("关闭弹窗投递失败: {e}");
            }
            send_json(req, 200, r#"{"ok":true}"#.into());
        }
        _ => {
            let body = serde_json::to_string(&ErrorResponse { ok: false, error: "not found".into() })
                .unwrap_or_default();
            send_json(req, 404, body)
        }
    }
}

fn handle_notify(cfg: &Config, mut req: Request) {
    let mut body = String::new();
    let mut reader = req.as_reader().take(MAX_BODY as u64 + 1);
    if reader.read_to_string(&mut body).is_err() {
        send_error(req, &api::NotifyError::BadJson("请求体不是有效的 UTF-8 文本".into()));
        return;
    }
    if body.len() > MAX_BODY {
        send_error(req, &api::NotifyError::BadJson("请求体过大".into()));
        return;
    }
    let parsed: api::NotifyRequest = match serde_json::from_str(&body) {
        Ok(p) => p,
        Err(e) => {
            send_error(req, &api::NotifyError::BadJson(e.to_string()));
            return;
        }
    };
    if let Err(err) = parsed.validate() {
        send_error(req, &err);
        return;
    }

    let via = notify::dispatch(cfg, &parsed);
    let resp = NotifyResponse { ok: true, via };
    log::info!(
        "通知已投递(via={}): {}",
        via.as_str(),
        parsed.title.chars().take(50).collect::<String>()
    );
    match serde_json::to_string(&resp) {
        Ok(b) => send_json(req, 200, b),
        Err(_) => send_error(req, &api::NotifyError::BadJson("响应序列化失败".into())),
    }
}
