// UI 编译失败即构建失败,panic 是 build script 的标准失败方式
#![allow(clippy::unwrap_used, clippy::panic)]

fn main() {
    slint_build::compile("ui/popup.slint").unwrap();

    let target = std::env::var("TARGET").unwrap_or_default();

    // Windows:嵌入图标资源(交叉编译 windres;文件管理器/任务栏显示)。
    // 本地无 windres(纯 cargo check 场景)跳过并告警,CI 打包机恒有 mingw 工具链
    if target.contains("windows")
        && let Err(e) = winresource::WindowsResource::new()
            .set_icon("assets/icons/x-notify-service.ico")
            .compile()
    {
        if e.kind() == std::io::ErrorKind::NotFound {
            println!("cargo:warning=未找到 windres,跳过图标嵌入(本地开发环境)");
        } else {
            panic!("图标资源编译失败: {e}");
        }
    }

    // Linux 多档窗口图标:构建期把 PNG 解码并直接产出 X11 线序字节流,
    // 运行时零解码零拷贝(依赖树的 png 仅存在于构建图)
    if target.contains("linux") {
        let mut out = String::from(
            "/// 构建期生成:_NET_WM_ICON 线序负载(小端 u32 流,含各档宽高)\npub(super) const ICON_BLOB: &[u8] = &[\n",
        );
        for size in [128u32, 48, 32, 16] {
            let path = format!("assets/icons/hicolor/x-notify-service-{size}.png");
            let data = std::fs::read(&path).unwrap();
            let decoder = png::Decoder::new(data.as_slice());
            let mut reader = decoder.read_info().unwrap();
            let mut buf = vec![0u8; reader.output_buffer_size()];
            let info = reader.next_frame(&mut buf).unwrap();
            for v in [info.width, info.height] {
                for b in v.to_le_bytes() {
                    out.push_str(&format!("{b},"));
                }
            }
            let (chunks, _) = buf.as_chunks::<4>();
            for px in chunks {
                let [r, g, b, a] = *px;
                let v = (u32::from(a) << 24)
                    | (u32::from(r) << 16)
                    | (u32::from(g) << 8)
                    | u32::from(b);
                for b in v.to_le_bytes() {
                    out.push_str(&format!("{b},"));
                }
            }
            out.push('\n');
            println!("cargo:rerun-if-changed={path}");
        }
        out.push_str("];\n");
        std::fs::write(
            format!("{}/window_icons.rs", std::env::var("OUT_DIR").unwrap()),
            out,
        )
        .unwrap();
    }

    // 内嵌演示页用的 SDK:dist 存在则复制,否则写占位(纯 cargo build 不依赖 pnpm;
    // 正式打包流程 pack-linux/pack-windows 均先构建 SDK 再 cargo build,嵌入的是真产物)
    let sdk_dist = std::path::Path::new("sdk/js/packages/sdk/dist/x-notify-service-sdk.js");
    let content = std::fs::read_to_string(sdk_dist).unwrap_or_else(|_| {
        "// sdk.js 占位:先在 sdk/js 下 pnpm -F @hexinfo/x-notify-service-sdk build".to_string()
    });
    let out = std::env::var("OUT_DIR").unwrap();
    std::fs::write(format!("{out}/embedded-sdk.js"), content).unwrap();
    println!("cargo:rerun-if-changed={}", sdk_dist.display());
}
