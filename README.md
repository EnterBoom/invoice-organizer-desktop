# 发票整理器

一个基于 Tauri 2、React 和 Rust 的跨平台桌面工具，用于批量预览、重命名和归档本地发票文件。

## 支持平台

- macOS：Apple Silicon 与 Intel 通用安装包
- Windows：Windows 10/11 x64 安装包

## 主要功能

- 扫描 PDF、OFD、JPG、JPEG、PNG、WEBP、HEIC 发票文件
- 从文件名提取日期、金额、开票主体和票种信息
- 按月份和票种生成归档目录
- 执行前展示完整预览，不直接修改原文件
- 支持复制归档或移动归档
- 自动处理重名文件
- 保存常用设置

## 下载成品

进入仓库右侧的 **Releases**，下载对应平台的安装包：

- macOS：`.dmg`
- Windows：`.msi` 或 `.exe`

macOS 首次打开若被系统拦截，可在“系统设置 → 隐私与安全性”中允许打开。

## 本地开发

需要安装：

- Node.js 20 或更高版本
- Rust stable
- Windows 额外需要 Microsoft C++ Build Tools 与 WebView2 Runtime

安装依赖并启动：

```bash
npm install
npm run tauri dev
```

构建当前平台安装包：

```bash
npm run tauri build
```

构建产物位于 `src-tauri/target/release/bundle/`。

## 自动构建发布

推送 `v*` 版本标签后，GitHub Actions 会同时在 macOS 和 Windows 编译机上构建安装包，并上传到同一个 GitHub Release。

示例：

```bash
git tag v0.1.0
git push origin v0.1.0
```

## 数据位置

软件只处理用户选择的本地文件夹。设置保存在：

- macOS：`~/.codex/invoice-organizer/settings.json`
- Windows：`%APPDATA%\invoice-organizer\settings.json`

