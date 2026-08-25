#!/usr/bin/env bash
# 打包 Linux 交付物(正式):tgz 绿色版(bin + config + demo + sdk + install.sh + 图标)
# 全程普通用户权限:install.sh 复制到 ~/.local/bin 并调用 x-notify-service install 注册
#
# 构建方式:
#   macOS(Apple Silicon):Docker 双平台容器各跑原生编译(arm64 原生 / amd64 Rosetta),
#                         容器内 apt 安装 fontconfig-dev,避免一切交叉编译工具链
#   Linux 原生:直接 cargo build(需 libfontconfig1-dev)
# 用法: scripts/pack-linux.sh [arch]
#   arch ∈ x86_64(默认) | aarch64 | all
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="$(grep -m1 '^version' Cargo.toml | sed 's/version = "\(.*\)"/\1/')"
ARCHES="${1:-x86_64}"

# 宿主机统一构建 JS(容器只负责编译二进制)
build_js() {
    (cd sdk/js && pnpm install --silent && pnpm -F @hexinfo/x-notify-service-sdk build)
}

# 容器内原生编译:platform = docker 平台,dir = target 子目录
build_in_docker() { # $1=platform $2=archname
    docker run --rm --platform "$1" -v "$PWD":/work -w /work \
        -v "$HOME/.cargo/registry:/usr/local/cargo/registry" \
        -e http_proxy=http://host.docker.internal:7890 \
        -e https_proxy=http://host.docker.internal:7890 \
        -e HTTP_PROXY=http://host.docker.internal:7890 \
        -e HTTPS_PROXY=http://host.docker.internal:7890 \
        -e no_proxy=localhost,127.0.0.1 \
        -e CARGO_TARGET_DIR="/work/target/linux-$2" \
        rust:1-slim sh -c \
        'sed -i "s|http://deb.debian.org|https://deb.debian.org|g" /etc/apt/sources.list.d/*.sources 2>/dev/null || true; apt-get update -qq || true; apt-get install -y -qq libfontconfig1-dev pkg-config || true; pkg-config --exists fontconfig || { echo "fontconfig 安装失败" >&2; exit 1; }; cargo build --release'
}

# 组装单个架构的 tgz:$1=archname(x86_64/aarch64) $2=二进制路径
assemble() {
    local arch="$1" bin="$2"
    local pkg="x-notify-service-${VERSION}-linux-${arch}"
    local out="dist/${pkg}"
    rm -rf "$out"
    mkdir -p "$out/bin" "$out/config"
    cp "$bin" "$out/bin/x-notify-service"
    cp scripts/templates/config.toml "$out/config/"
    cp scripts/templates/install-linux.sh "$out/install.sh"
    chmod +x "$out/install.sh"
    cp assets/demo.html "$out/demo.html"
    cp sdk/js/packages/sdk/dist/x-notify-service-sdk.js "$out/sdk.js"
    cp -R assets/icons/hicolor "$out/icons/"
    tar -czf "dist/${pkg}.tar.gz" -C dist "$pkg"
    rm -rf "$out"
    ls -lh "dist/${pkg}.tar.gz" | awk '{print "==> 产出:", $5, $9}'
}

want() { [ "$ARCHES" = "all" ] || [ "$ARCHES" = "$1" ]; }

build_js

if [ "$(uname -s)" = "Darwin" ]; then
    if want x86_64; then
        echo "==> Docker(amd64/Rosetta)原生编译 x86_64"
        build_in_docker linux/amd64 x86_64
        assemble x86_64 "target/linux-x86_64/release/x-notify-service"
    fi
    if want aarch64; then
        echo "==> Docker(arm64/原生)编译 aarch64"
        build_in_docker linux/arm64 aarch64
        assemble aarch64 "target/linux-aarch64/release/x-notify-service"
    fi
else
    # Linux 原生:按本机架构构建
    local_arch="$(uname -m)"   # x86_64 / aarch64
    echo "==> 原生编译 ${local_arch}"
    cargo build --release
    assemble "${local_arch}" "target/release/x-notify-service"
fi
