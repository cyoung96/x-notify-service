# x-notify-service

跨平台消息通知服务:浏览器网页调用 → 屏幕右下角置顶弹窗(系统通知兜底)。
面向 Windows / Linux(UOS、麒麟)交付,macOS 仅用于开发测试。是 dop-imgctls 老通知方案的现代化替代(绿色版、免提权、低内存)。

```
┌──────────┐  HTTP(127.0.0.1)  ┌─────────────────┐
│ 业务页面  │ ───────────────→ │ x-notify-service │──→ 右下角置顶弹窗(Slint 软件渲染)
│ JSSDK    │ ←── /health 验身 └─────────────────┘└─→ 系统通知兜底(弹窗不可用时)
└──────────┘        服务不在线 → x-notify:// 协议拉起
```

## 服务端(Rust)

### 运行与安装

```
x-notify-service              # 运行服务(常驻进程;无参数即此模式)
x-notify-service install      # 注册开机自启 + x-notify:// 协议,并启动服务
x-notify-service uninstall    # 清理全部注册项
x-notify-service info         # 诊断快照:实例/端口/显示环境/工作区/注册状态(只读)
x-notify-service start|stop|restart   # 服务生命周期(幂等;stop 经端口文件 pid + 身份校验)
x-notify-service notify -t "标题"                      # 本机手测一条通知(不经 HTTP)
x-notify-service notify -t "标题" -b "<b>正文</b><br>第二行"   # 正文支持 HTML 子集(加粗/颜色/字号/br)
x-notify-service notify -t "标题" -f                   # 同上,强制走系统通知兜底(正文自动剥为纯文本)
x-notify-service close        # 关闭当前弹窗(幂等)
```

- 全部注册均为**用户级**(Windows HKCU / macOS LaunchAgent / Linux XDG),无需管理员/root。
- 单实例:重复启动静默退出;端口默认 `17320`,被占自动向后探测 10 个。
- 日志:按天滚动保留 7 天;Linux `~/.local/state/x-notify-service/logs`、macOS `~/Library/Logs/x-notify-service`、Windows `%LOCALAPPDATA%\x-notify-service\logs`。
- 配置文件查找顺序:`--config` > 二进制同目录 `config.toml`(绿色版友好)> 用户配置目录(模板见 `scripts/templates/config.toml`)。

### HTTP API(127.0.0.1,无鉴权,CORS 全开)

```
GET  /health → {"app":"x-notify-service","version":"0.1.0","port":17320}
POST /notify → {"ok":true,"via":"popup"|"system"}
     body: {"title":"必填,≤200字", "body":"可选,≤2000字,HTML子集"}
```

正文 HTML 子集:`<b>/<strong>` 加粗、`<font color="…">/<span style="color:…">` 颜色、`<font size="16">/<span style="font-size:16px">` 字号(11~18,按行生效)、`<br>` 换行、HTML 实体;其余标签剥除保留内文,最多显示 4 行(超出截断加 …),行距 1.5 倍。弹窗常驻不超时:点击关闭或被新通知顶掉;同时只有一条,不堆叠。

### 进程模型

主线程 = Slint GUI 事件循环;1 个 HTTP 工作线程。无弹窗环境(无 DISPLAY / `--no-popup`)自动降级为系统通知(notify-rust),响应 `via:"system"`。

## JSSDK(sdk/js,pnpm workspace)

```ts
import { createNoticeBridge } from '@hexinfo/x-notify-service-sdk'

// 仅对需要通知能力的角色初始化(接入方条件接入)
const bridge = createNoticeBridge()          // basePort=17320, portRange=10
await bridge.discover()                       // 探测服务 → baseUrl | null

// 静默失败语义:服务未安装/未运行时不拉起、不抛错,返回 { ok: false } 由业务自理
const r = await bridge.notify({
  title: '工单提醒',
  body: '<font size="16"><b>紧急工单</b></font><br>张三提交了<font color="#d93025">紧急</font>归档申请',
})
if (!r.ok) { /* 页面内降级提示,或引导安装 */ }

// 页面初始化时提前拉起服务(对接入角色),不等通知时刻才冷启动
await bridge.start()                              // 幂等;未安装/超时静默返回 false
await bridge.close()                              // 显式关闭当前弹窗(对应老系统 ClosePopup)
```

- 零依赖、纯 ESM、Vite lib 模式构建,`target: chrome87`(信创浏览器基线)。
- 零预检:notify 不带 Content-Type(默认 text/plain),属 CORS 简单请求。
- 发现策略:首次并发探测 `[17320, 17329]` 后缓存;`/health` 中 `app` 标识验明正身;仅请求失败才重新探测。

开发:

```
cd sdk/js
pnpm install && pnpm build && pnpm lint && pnpm typecheck
pnpm demo        # 演示页 http://localhost:8086
```

## 打包(同构脚本)

```
scripts/pack-linux.sh    [target]   # 正式:tgz 绿色版(bin+config+demo+sdk+install.sh+图标)
                                    # 默认 x86_64;飞腾/鲲鹏传 aarch64-unknown-linux-gnu;
                                    # macOS 上经 Docker(rust:1-slim)交叉编译
scripts/pack-windows.sh  [target]   # 正式:NSIS setup.exe(per-user 免 UAC、可选目录、LZMA)
scripts/pack-macos.sh              # 仅测试:.app bundle(验证 x-notify:// 协议拉起)
```

- Linux 安装(全程普通用户权限):解压 tgz → `./install.sh` → 复制到 `~/.local/bin`、
  图标到 `~/.local/share/icons`、演示页与 SDK 到 `~/.local/share/x-notify-service/`,
  并调用 `x-notify-service install` 完成用户级注册(自启动 + 协议)+ 分离启动服务。
- Windows 安装:双击 setup.exe → 选目录 → 安装末尾静默注册并启动服务。

## 已知限制

- Wayland 会话下弹窗无法自定位右下角,自动走系统通知兜底(X11 正常)
- macOS 协议拉起依赖 .app bundle(非正式交付平台)
- Windows 兜底系统通知来源默认显示为 PowerShell(AppId 机制),`--app-id` 可自定义
- 富文本正文不支持表格/图片/复杂 CSS(设计取舍:零依赖、低内存)
