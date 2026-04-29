
# HecO

**The HarmonyOS app development CLI tool built for you and AI agents.**

[![Rust](https://img.shields.io/badge/rust-1.60%2B-blue.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows-lightgrey.svg)]()
[![Homebrew](https://img.shields.io/badge/homebrew-tap-orange.svg?logo=homebrew)](https://github.com/heco-cli/homebrew-tap)
[![Winget](https://img.shields.io/badge/winget-HecO--CLI.HecO-blue.svg?logo=windows)](https://github.com/microsoft/winget-pkgs/tree/master/manifests/h/HecO-CLI/HecO)


`HecO`是用于鸿蒙/HarmomyOS应用开发的命令行工具，并提供便捷的命令行提示及补全，方便鸿蒙应用开发者和AI调用。

> **⚠️ 注意**：`HecO` 处于快速迭代阶段，命令行参数及输出格式可能会频繁更新。强烈建议在终端中使用 `heco --help` 或依赖 Shell 自动补全功能，来获取最新、最准确的参数格式和使用说明。

## ✨ 核心特性

- 🚀 **极速构建 (`build`)**：自动推断工程结构，支持 `module@target` 粒度构建及多模块同时构建，完美集成 `hvigor` 且支持多产品 (`--products`) 一键循环构建 `assembleApp`。
- 📦 **智能运行 (`run`)**：内置日志追踪，自动解析依赖并在推送 HAP 的同时连带安装相关 HSP，过滤 `hilog` 及 `FaultLogger` 精准捕获崩溃。
- 🧹 **一键清理 (`clean`)**：支持工程或单模块清理，同时提供 `--with-devices` / `--with-all-devices` 参数快速卸载远端设备上的应用。
- 📱 **设备与模拟器管理 (`device`** **/** **`emulator`)**：跨平台（macOS/Windows）快速启动、停止模拟器实例，列出可用物理设备及模拟器。
- 💡 **终端自动补全 (`completion`)**：提供 zsh/bash 等 Shell 的动态命令和参数补全（如根据逗号分隔动态提示可用的模块、模拟器或设备名称）。
- 🛠 **跨平台路径适配**：智能推断并解析 DevEco Studio 在 macOS 和 Windows 上的安装及 SDK 路径。

## 📥 安装

### 前置条件

- DevEco Studio

### 使用 Homebrew 安装（macOS 推荐）

```bash
brew tap heco-cli/tap
brew install heco
```

### 使用 Winget 安装（Windows 推荐）

```powershell
winget install HecO-CLI.HecO -s winget
```

### 从源码安装

依赖`Rust`环境

```bash
git clone git@github.com:heco-cli/heco.git
cd heco
cargo install --path .
```

安装完成后，在终端运行 `heco --help` 验证是否安装成功。

## 🚀 快速开始

### 1. 初始化配置(可选，仅当DevEco Studio安装路径并非默认路径时或者需要多sdk切换时使用)

```bash
heco env --help
```

按照向导完成 DevEco Studio 基础配置的设定。

### 2. 构建工程

```bash
# 构建整个工程
heco build

# 构建特定模块及其 target
heco build --modules entry@default

# 构建多个模块
heco build --modules entry,feature

# 一键构建所有 product 的 APP 包
heco build --products
```

### 3. 运行应用与日志追踪

```bash
# 运行当前应用并在前台实时追踪 hilog
heco run

# 仅查看 warning 级别以上日志 (自动映射为 W,E,F)
heco run --app-log-level warning

# 在指定设备上后台拉起应用，不追踪日志
heco run --device emulator-5554 --daemon
```

### 4. 模拟器管理

```bash
# 查看可用模拟器列表
heco emulator list

# 启动指定的模拟器
heco emulator start my-emulator
```

### 5. 终端补全设置

#### Zsh 环境&#x20;

```bash
# 追加补全配置到 ~/.zshrc
cat << 'EOF' >> ~/.zshrc

# HecO complete env 
autoload -Uz compinit; compinit 
source <(heco completion zsh) 
EOF

# 使配置生效
source ~/.zshrc
```

#### Bash 环境

```bash
# 追加补全配置到 ~/.bashrc
cat << 'EOF' >> ~/.bashrc

# HecO complete env 
eval "$(heco completion bash)"
EOF

# 使配置生效
source ~/.bashrc
```

## ⚙️ 配置说明

HecO 配置文件采用 TOML 格式（强制 **kebab-case** 规范）。

**配置路径**：`~/.config/heco/config.toml`

可配置多个`DevEco Studio`路径，`heco`执行时会通过`targetSdkVersion`选择合适的版本

通过`heco env`管理配置文件，无需手动修改配置文件

## ⌨️ 命令速查手册

- `env` - 管理多个`DevEco Studio`环境
- `build` - 编译构建模块及 APP 产品包
- `clean` - 清理构建产物并可选择卸载设备应用
- `lint` - 运行 `codelinter`进行代码风格检查
- `emulator` - 模拟器实例管理 (start/stop/list)
- `run` - 在设备或模拟器上运行应用，并追踪全生命周期日志
- `device` - 物理设备与模拟器设备连接状态管理
- `completion` - 生成对应 Shell 环境的自动补全脚本

## 🤝 参与贡献

欢迎提交 Issue 和 Pull Request！

## 📄 许可证

本项目基于 [Apache 2.0 许可证](LICENSE) 开源。
