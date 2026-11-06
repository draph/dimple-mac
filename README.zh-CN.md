# Folderify Desktop

[English](README.md) | 简体中文

macOS 文件夹图标工具，使用 Tauri v2、React、TypeScript、Vite 和 Tailwind。
独立维护的 Folderify 图形界面项目（folderify-gui），以 [lgarron/folderify](https://github.com/lgarron/folderify) 为参考，并非其官方桌面版本。
目前是 T01 工程原型：只有启动面板和只读原生连接检查，尚不能设置文件夹图标。

## 安装与使用状态

目前没有可供普通用户安装的正式版本，需按下文从源码启动。原型只显示工程状态，尚不能选择目录、预览或应用图标，不应当作可用的图标工具。

尚未接入 Finder 读取，不会申请 Finder 自动化权限；后续支持首次实际读取时申请，拒绝后仍可手动选择。当前原型面板与原生菜单已包含简体中文和英文资源；启动时跟随 macOS 应用语言，无匹配时回退英文。修改系统中的应用语言后重新启动生效，不提供应用内语言设置。后续功能的英文翻译仍在 T18-L 验收。

## 开发

本机：Apple Silicon（arm64）、macOS 26.6.2、Xcode 27.2（SDK 27.2）、Node 24.18.0、npm 12.2.2、Rust 1.99.1。
项目目录名为 `folderify-desktop`，应用显示名为 Folderify Desktop。
Rust 版本由 `rust-toolchain.toml` 固定，依赖由 npm／Cargo lockfile 锁定。

```sh
# 首次安装开发依赖；已有 Rust 可跳过工具链安装
# Rust 通过 https://rustup.rs/ 安装，再加载用户级环境
source "$HOME/.cargo/env"
npm ci
git submodule update --init --recursive
npm run tauri dev
```

```sh
npm run typecheck
npm test
npm run build
npm run check:rust
npm run tauri build -- --no-bundle
# 后续交付阶段：安装两个 Rust target 后构建 Universal 应用和 DMG
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri build -- --target universal-apple-darwin
```

`npm run check:rust` 执行 fmt、Clippy（警告视为错误）和单元测试。前端检查同时覆盖 UI 与 Vite 配置。
`npm test` 使用 Node 24 内置 TypeScript 支持，检查本地化键、命名参数、回退及错误显示；Rust 测试覆盖会话／错误序列化和 locale 归一化。前端与原生菜单共用 `src/locales/` 资源。原生错误采用 `code`、`params` 和可选 `targetId`，不向用户显示原始拒绝字符串。
所有文件夹写入测试必须仅使用独立临时目录。

## 上游与资源

只读参考子模块：`upstream/folderify`，来源 https://github.com/lgarron/folderify 。
固定提交：`16de35dd0ee94be4eae046dde709949b5d779e48`。
真实模板目录：`src/resources/folders/`，包括 Big Sur 和 Tahoe 的多尺寸 iconset。
仓库 MIT 声明不能单独证明 Apple 图像再分发授权，默认模板仍待确定，当前应用不引用这些资源。

## 工程原型验证（2026-11-06）

以下为 T01 历史验证；本轮改名后的检查结果另记于任务文档。构建产物名称已随项目改名更新。

- `npm run typecheck`、`npm run build`、`cargo check --manifest-path src-tauri/Cargo.toml` 通过。
- `npm run check:rust` 通过：fmt、Clippy（零警告）、1 个单元测试。
- `npm run tauri dev` 编译并启动；已配置开发专用 CSP，允许本机 Vite 热更新连接。
- `npm run tauri build -- --debug --bundles app` 通过；调试包在 `src-tauri/target/debug/bundle/macos/Folderify Desktop.app`。
- 原生 UI 检查确认本地 `tauri://localhost` 页面显示“原生服务已连接 · v0.1.0”。
- 调试应用 28.36 MiB，不作为 release 体积或最终交付验收结果。

## 当前限制

- 目标选择、图标生成、菜单栏、Finder 集成和设置／恢复图标尚未实现。
- 配置中的 macOS 10.15 是框架构建候选下限，不代表已经验证支持；目前只有本机环境可供验证。
- 尚无 Intel／最低版本运行、Universal 构建、性能、签名、公证或安装包验收证据。
- 不公开发布，不自动请求 Finder 权限。当前默认应用图标来自官方工程模板，产品图标尚未确定。

## 卸载开发原型

停止 `npm run tauri dev` 并删除自行复制的调试应用包即可。原型尚不保存目标、素材或设置，也没有实现文件夹图标写入。源码和开发工具链独立管理，删除应用不会自动删除它们。

## 许可

项目代码采用 MIT，见 LICENSE。上游及第三方素材需分别核实，安装包的第三方声明在打包阶段补齐。
