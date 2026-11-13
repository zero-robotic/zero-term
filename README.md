# Zero Term

基于 [zero-ui](https://github.com/zero-robotic/zero-ui) 的原生多 Tab 终端，使用 `alacritty_terminal 0.26` 处理终端协议、字符网格、滚动历史和 PTY。

## 运行

安装当前稳定版 Rust，然后执行：

```sh
cargo run --locked
```

zero-ui 固定到 GitHub 提交 `c0402d47c7a2945d2155a27ee7d2285727a6d558`，无需旁边存在 zero-ui 源码目录。首次构建需要访问 GitHub 和 crates.io；如果 Cargo 的内置 Git 下载失败，可设置 `CARGO_NET_GIT_FETCH_WITH_CLI=true`。

macOS 可以生成应用包后双击启动：

```sh
./scripts/bundle-macos.sh
open "target/Zero Term.app"
```

应用包为本地开发构建，未签名、公证或安装到系统目录。

## 功能

- 每个 Tab 独立运行默认 Shell，拥有自己的 PTY、屏幕、选区和 10,000 行滚动历史。
- Tab 显示当前前台进程的短名称（如 `zsh`、`vim`、`python3`），每 250ms 更新；超过 12 个字符时截短并加省略号。macOS/Linux 从 PTY 前台进程组读取，忽略 Shell 的 OSC 路径标题。
- 点击 `+` 新建 Tab，点击标签切换，点击 `×` 关闭。关闭最后一个 Tab 时退出窗口。
- Shell 退出后保留屏幕并标注 `done`，方便查看最后输出。
- 窗口尺寸变化同步到所有 Tab 的网格和 PTY。
- ANSI 16 色、256 色、真彩色、反色、下划线、删除线，Unicode 宽字符及组合字符。
- 鼠标拖动选区，系统剪贴板复制/粘贴，bracketed paste。
- 方向键、Ctrl/Alt 组合键、Home/End、PageUp/PageDown、Insert/Delete、F1–F12。
- 滚轮历史、备用屏幕、应用光标键，以及传统/SGR 鼠标事件。
- 输入法提交和预编辑显示，提供候选窗口光标位置。

| 操作 | macOS | 通用快捷键 |
| --- | --- | --- |
| 新建 Tab | Cmd+T | Ctrl+Shift+T |
| 关闭 Tab | Cmd+W | Ctrl+Shift+W |
| 下一个 / 上一个 Tab | Ctrl+Tab / Ctrl+Shift+Tab | 同左 |
| 选择第 1–9 个 Tab | Cmd+1…9 | Ctrl+Shift+1…9 |
| 复制选区 | Cmd+C | Ctrl+Shift+C |
| 粘贴 | Cmd+V | Ctrl+Shift+V |
| 滚动历史 | Shift+PageUp / PageDown | 同左 |

在启用鼠标协议的终端程序中，按住 Shift 可以使用本地文本选择和历史滚动。普通 Ctrl+C 仍发送中断到 Shell。

## 字体

自动查找 Menlo、DejaVu Sans Mono、Liberation Mono 或 Consolas。可指定字体文件：

```sh
ZERO_TERM_FONT=/path/to/monospace.ttf \
ZERO_TERM_FALLBACK_FONT=/path/to/cjk.ttc \
cargo run --locked
```

中文回退字体默认查找 Hiragino Sans GB、Noto Sans CJK 或微软雅黑。字体从本机读取，不随项目分发。

## 验证

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo run --locked -- --smoke-ms 1500
```

测试覆盖实际 Shell 输出与 PTY resize、键盘编码、安全边界内的粘贴编码、多 Tab 状态隔离和关闭、ANSI/中文选区及绘制场景校验。PTY 测试需要 `/bin/sh`，绘制测试需要上述字体之一。窗口 smoke test 需要图形桌面与运行登录 Shell 的权限。

已在 macOS 实际窗口验证新建、切换、关闭 Tab、命令粘贴执行、ANSI 颜色及中文显示。Linux/Windows 尚未进行运行验证；Windows 暂未接入前台进程名查询。

## 实现与边界

- `src/app.rs`：zero-ui `Application`、Tab 状态、输入路由、场景绘制。
- `src/terminal.rs`：Alacritty PTY 线程、事件队列、生命周期及尺寸同步。
- `src/input.rs`：按键/粘贴编码。
- `src/text.rs`：zero-ui Parley 字体与文本布局缓存。
- `src/palette.rs`：终端颜色映射。

使用 zero-ui CPU 渲染后端。UI 每 16ms 检查合并的终端更新标记，仅在内容、交互或光标状态改变时请求重绘。终端输出解析在 PTY 后台线程执行。

当前版本未实现分屏、会话持久化、搜索、标签重排、图片协议、独立粗体/斜体字形和无障碍终端文本树。OSC 52 不接入系统剪贴板；复制/粘贴由用户快捷键触发。关闭 Tab 会结束该 Shell，终端历史不持久化。
