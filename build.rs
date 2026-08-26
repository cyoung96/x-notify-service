// UI 编译失败即构建失败,panic 是 build script 的标准失败方式
#![allow(clippy::unwrap_used)]

fn main() {
    slint_build::compile("ui/popup.slint").unwrap();
}
