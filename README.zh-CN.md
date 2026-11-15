# Dimple — Folder icons for Mac

[English](README.md) | 简体中文

一款 macOS 菜单栏工具，为文件夹添加具有原生凹陷质感的自定义图标。选中文件夹、选择徽标、应用，在同一个面板完成。

Dimple 使用 Tauri v2、Rust、React、TypeScript、Vite 和 Tailwind。

## 功能

- 为单个或每次最多 100 个文件夹应用图标，不修改文件夹正文。
- 选择内置徽标、导入 PNG、静态 SVG 或 JPG/JPEG，或输入 1–4 个可见字符。
- 实时预览，可按需调整缩放、位置和文件夹颜色。
- 通过菜单栏、全局快捷键（默认 **⌥F**）、Finder 服务或拖入文件夹打开面板。
- 确认后恢复系统默认图标；覆盖和恢复均不可撤销。
- 离线使用，无账号、遥测或上传。

## 安装

Dimple 以面向 Apple Silicon 和 Intel Mac 的 Universal DMG 分发，需要 macOS 13.3 或更高版本。项目仅在 macOS 26.6.2 Apple Silicon 上验证过；Intel Mac 和 macOS 13.3–25 未经测试。

从 [GitHub Releases](https://github.com/draph/dimple-mac/releases) 下载 DMG，打开后将 **Dimple** 拖入**应用程序**。无需安装 Python、ImageMagick 或开发工具。

应用采用 ad-hoc 签名，未公证。首次启动可能被 macOS 阻止：打开**系统设置 → 隐私与安全性**，找到 Dimple 的提示并选择**仍要打开**；较旧 macOS 可在 Finder 中按住 Control 点击 **Dimple**，再选择**打开**。不要关闭 Gatekeeper。可对照 Release 页面公布的 SHA-256 核对下载文件。

## 授权

首次读取 Finder 选中项时会请求 Finder 自动化权限；拒绝后仍可手动选择或拖入文件夹。应用不提权：目标须为可写文件夹，系统保护目录和应用包会被拒绝。

## 使用

1. 在 Finder 中选中文件夹，点击菜单栏图标或按 **⌥F** 打开面板。也可拖入文件夹、使用文件夹选择器或调用 Finder 服务。
2. 选择内置徽标、导入图片或输入文字，预览会在目标卡片内更新。
3. 点击**应用**；如果会覆盖已有自定义图标，需先确认。

超过 100 个文件夹、目录与徽标混合或全部无效的输入会被整体拒绝，并保留之前的目标。批量部分结果提供**重试失败项**、**继续剩余项**和**停止剩余项**；超时不代表写入已取消，需等待结果。

**恢复默认图标**让文件夹回到系统图标，不会找回之前的自定义图标。导入限制和被拒绝的 SVG 特性会在面板内提示，**调整**控制缩放、位置和文件夹颜色。

**设置**包含外观（跟随系统／深色／浅色）、语言（跟随系统／简体中文／English，启动时解析）、全局快捷键、当前版本，以及仅在点击时执行的**检查更新**。

## 限制与隐私

Dimple 完全离线工作：无账号、遥测或上传。唯一的联网是主动点击**检查更新**，它查询公开的 GitHub Releases API 并打开 Release 页，不下载或安装任何内容。

- 仅面向 macOS；采用 ad-hoc 签名、未公证，Intel Mac 与 macOS 13.3–25 未经测试（见安装）。
- 外接盘、网络盘和云同步目录取决于自身权限，图标可能不跨设备同步。
- 文件夹底图保持 Big Sur 浅色原生蓝，不随系统版本或界面深浅色变化；文件夹着色与深浅色相互独立。
- 写入使用 macOS 文件协调与身份复核；绕过协调的程序仍可能在事后替换路径。
- 不提供图标备份或撤销、PNG／ICNS 导出、编辑历史、预设管理、自动更新或插件市场。
- 目标路径、导入素材和编辑草稿不跨启动保存；快捷键、外观和语言偏好会保存。
- 本版本未做完整 VoiceOver 覆盖。

## 反馈

问题与功能建议请提交到 [GitHub Issues](https://github.com/draph/dimple-mac/issues)。报告时请附应用版本、macOS 版本和架构。

## 从源码构建

使用 Node.js 24、npm 12、`rust-toolchain.toml` 固定的 Rust 工具链，以及带 macOS SDK 的 Xcode 或 Command Line Tools。依赖由 npm 和 Cargo lockfile 固定。

```sh
git clone https://github.com/draph/dimple-mac.git
cd dimple-mac
source "$HOME/.cargo/env"
npm ci
npm run tauri:dev
```

执行检查并构建原生可执行文件（文件夹写入测试必须使用隔离临时目录）：

```sh
npm run typecheck
npm test
npm run build
npm run check:rust
npm run tauri build -- --no-bundle
```

构建 Universal 应用和 DMG：

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run build:universal
```

产物为 `src-tauri/target/universal-apple-darwin/release/bundle/` 下的 `macos/Dimple.app` 和 `dmg/Dimple_1.0.0_universal.dmg`。构建采用 ad-hoc 签名，不是 Developer ID 签名或公证：`codesign -dv` 显示 `Signature=adhoc`，`lipo -archs` 显示 `x86_64 arm64`。请从 Finder 或 `open path/to/Dimple.app` 启动，而不是直接运行 `Contents/MacOS` 内的可执行文件。应用标识为 `com.draph.dimple`。

## 卸载

如需恢复系统默认图标，请先在应用中执行恢复。从菜单栏退出 Dimple，再将其从**应用程序**移至废纸篓。删除应用不会影响已应用的图标，也不会恢复被覆盖的自定义图标。可选删除 `~/Library/Application Support/com.draph.dimple/shortcut.json` 和 `language.json`。

## 许可与上游

项目代码采用 [MIT 许可](LICENSE)。默认模板已内联到 `src/folder-templates/GenericFolderIcon.BigSur.iconset/`，逐字节来自固定的上游 `folderify` 提交 `16de35dd0ee94be4eae046dde709949b5d779e48`，逐文件哈希和来源记录在该目录。

该模板以上游固定仓库的 MIT 声明作为许可依据，项目对其保留的限制记录在[第三方声明](THIRD_PARTY_NOTICES.md)中；同一文件也随应用分发到 `Contents/Resources/THIRD_PARTY_NOTICES.md`。其他第三方素材保留各自许可。
