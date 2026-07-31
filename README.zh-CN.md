# [**SuperExplorer**](https://github.com/damody/SuperExplorer)

[English](README.md) | [繁體中文](README.zh-TW.md) | [简体中文](README.zh-CN.md)

使用 Rust 和 [GPUI-CE](https://github.com/gpui-ce/gpui-ce) 开发的 Windows 11 文件资源管理器。本项目结合了原生 Windows Shell 集成层，以及参考 Windows 文件资源管理器设计的自定义 GPUI 界面。

> 本项目仍在积极开发中，仅支持 Windows，尚未覆盖 Windows 文件资源管理器的全部 Shell 功能。

## 功能亮点

- 支持多标签页文件夹导航，以及后退、前进、向上、地址栏和搜索操作。
- 真实文件夹枚举、文件系统监视、排序和多种视图布局。
- 原生文件操作，包括新建、重命名、复制、移动、删除、冲突处理、取消和撤销日志。
- 集成 Windows 剪贴板、OLE 拖放、Shell 图标、叠加图标和原生上下文菜单。
- 优先探测索引搜索，不可用时改用有边界的文件系统搜索。
- 支持浅色、深色和高对比度主题、DPI 感知布局、键盘导航、输入法以及 UI Automation 语义。
- 提供单元、集成、架构、视觉、辅助功能、生命周期和 Windows 互操作验证脚本。
- 强化 Windows 拖放行为，修正 OLE 拖放状态转换，降低高频指针输入下的交互抖动。

## 系统要求

- Windows 11 x64。
- 支持 submodule 的 [Git](https://git-scm.com/)。
- Rust `1.85.0` 或更高版本，使用 `x86_64-pc-windows-msvc` 工具链。
- Visual Studio 2022 Build Tools 或 Visual Studio，并安装“使用 C++ 的桌面开发”工作负载。
- 与 MSVC 工具链兼容的 Windows SDK。
- 建议使用 PowerShell 7 运行验证脚本。

## 快速开始

克隆仓库并初始化 GPUI-CE submodule：

```powershell
git clone --recurse-submodules https://github.com/damody/file_explorer.git
cd file_explorer
```

如果克隆时未包含 submodule：

```powershell
git submodule update --init --recursive
```

构建并运行应用程序：

```powershell
cargo run -p explorer-app --locked
```

如需在启动时打开指定文件夹：

```powershell
$env:EXPLORER_INITIAL_PATH = 'D:\'
cargo run -p explorer-app --locked
```

## 发布版本构建

构建 Windows 可执行文件，并完成清单和版本资源处理：

```powershell
./scripts/finalize_windows_artifact.ps1 -Profile release
```

可执行文件将输出到 `target/release/SuperExplorer.exe`。

## 验证

运行主要仓库检查：

```powershell
cargo run -p explorer-uitest -- --suite quick
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Windows 图形界面和视觉检查需要交互式桌面会话。常用入口包括：

```powershell
./scripts/run_headful_validation.ps1 -SkipBuild -OutputDirectory target/headful-evidence/local
./scripts/capture_dpi_matrix.ps1 -OutputDirectory target/dpi-evidence/local
./scripts/check_architecture.ps1
./scripts/check_ui_tokens.ps1
```

完整流程请参阅[手动测试](docs/MANUAL_TESTS.md)和[视觉测试](docs/VISUAL_TESTING.md)。

## 工作区结构

| 路径 | 职责 |
| --- | --- |
| `crates/explorer-app` | 应用程序启动、Windows 前置要求和 GPUI 组合根节点 |
| `crates/explorer-common` | 共享诊断和错误类型 |
| `crates/explorer-jobs` | 后台任务协调 |
| `crates/explorer-model` | 导航、操作、窗口和领域模型 |
| `crates/explorer-search` | 查询解析和搜索引擎 |
| `crates/explorer-shell-win` | 原生 Windows Shell、剪贴板、OLE、图标和文件操作 |
| `crates/explorer-ui` | GPUI 界面、状态、布局、主题和交互 |
| `crates/explorer-test-support` | 共享测试夹具和辅助工具 |
| `vendor/gpui-ce` | 固定版本的 GPUI-CE Git submodule |
| `scripts` | 构建、冒烟测试、互操作、辅助功能和视觉验证脚本 |
| `docs` | 状态、证据、测试指南和实现说明 |

## 项目文档

- [当前状态](docs/STATUS.md)
- [最终交接与已知缺口](docs/FINAL_HANDOFF.md)
- [对等性矩阵](docs/PARITY_MATRIX.md)
- [检查点证据](docs/CHECKPOINT_EVIDENCE.md)
- [实现计划](docs/IMPLEMENTATION_PLAN.md)

## 已知限制

- 应用程序目前仅支持 Windows。
- 当前实现以文件系统为主；完整 Shell 命名空间、缩略图与预览处理程序，以及由 Broker 隔离的第三方扩展仍属于后续强化工作。
- 部分 OLE 拖放、混合 DPI、讲述人以及 Explorer 到应用程序的场景，需要在真实的交互式 Windows 桌面上手动验证。
- 搜索可用性和行为取决于 Windows Search 配置；索引搜索不可用时，应用程序会使用有边界的后备搜索。

详细验证状态和剩余缺口请参阅[最终交接文档](docs/FINAL_HANDOFF.md)。

## 许可与贡献

SuperExplorer 是**专有、Source Available（源代码可查看）软件**，不是开源软件。您仅可依下列适用文件查看源代码、准备贡献或开发兼容插件；禁止未经授权再分发、发布修改后的核心版本或商业利用核心程序。

- [最终用户许可协议](EULA.zh-CN.md)
- [Plugin SDK 许可条款](PLUGIN-SDK-LICENSE.zh-CN.md)
- [贡献指南](CONTRIBUTING.zh-CN.md)
- [贡献者许可协议](CLA.zh-CN.md)
- [插件发布协议](PLUGIN-PUBLISHING-AGREEMENT.zh-CN.md)

包括 `vendor/`、`third_party/` 及 `build/tools/` 下材料在内的第三方组件，仍依其各自的许可和 NOTICE 文件规范。
