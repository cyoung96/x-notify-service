// UI 编译失败即构建失败,panic 是 build script 的标准失败方式
#![allow(clippy::unwrap_used)]

fn main() {
    slint_build::compile("ui/popup.slint").unwrap();

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
