# 参与 SuperExplorer 贡献

生效日期：2026 年 7 月 31 日  
项目所有人：Damody  
语言：[English](CONTRIBUTING.md) | [繁體中文](CONTRIBUTING.zh-TW.md) | **简体中文**

> SuperExplorer 是专有、Source Available 软件，不是开源项目。本指南是商业法律与工作流程范本，正式发布前应由合格律师审阅。

## 1. 准据文本

三个语言版本应具有相同含义。如有差异，以本简体中文版本为准。

## 2. 参与方式

您可以报告问题、提出设计、改进文档、准备核心 Pull Request，或建立独立插件。查看源代码或参与项目不授予再分发 SuperExplorer 的一般权利；源代码访问仍适用 [EULA](EULA.zh-CN.md)。

## 3. 核心贡献与插件分流

对 SuperExplorer 核心、代码仓库自有 SDK 材料、文档、测试或项目资产的变更，属于“核心贡献”。使用公开接口而独立维护的扩展功能属于“插件”，适用 [Plugin SDK License](PLUGIN-SDK-LICENSE.zh-CN.md)。除非 Damody 要求，请勿将整个独立插件作为核心提交。

## 4. 合并前必须接受 CLA

每项核心贡献均须事先接受[贡献者许可协议](CLA.zh-CN.md)。在接受记录可归属于贡献者 GitHub 身份前，不得合并 Pull Request。公司贡献者在必要时须取得雇主授权。

在 CLA 自动签署功能提供前，请在 Pull Request 说明中加入以下完整声明：

> 我已阅读并接受 CLA.zh-CN.md（包括准据文本及争议条款），并有权依该条款提交本贡献。

Pull Request 历史应记录贡献者 GitHub 账户、Pull Request URL、Commit ID、声明及接受时间。对重大或公司贡献，Damody 可要求另行签署文件。

## 5. 提交 Pull Request 前

1. 搜索现有 Issue、OpenSpec change 及 Pull Request。
2. 重大行为或架构变更应先讨论。
3. 保持变更聚焦，避免无关格式调整或重构。
4. 新增或更新测试及用户文档。
5. 披露所有第三方代码、生成内容、AI 辅助内容、资产、工具及许可。
6. 确认未包含机密、雇主所有、非法复制或受限制的材料。

## 6. Pull Request 内容

每个 Pull Request 必须说明问题、解决方案、重要设计选择、完成的验证、兼容性影响及第三方材料。Commit 必须可审核，且不得包含凭证、个人数据、构建输出或无关文件。Damody 可要求修改、拆分工作，或在未接受的情况下关闭提案。

## 7. 审核与接受

提交不要求 Damody 审核、合并、发布、维护、署名或付款。仅在 Damody 合并或书面确认时才构成接受。Damody 可依 CLA 修改、组合、再许可、商业化或日后移除已接受的核心贡献。

## 8. 插件发布

独立插件作者保留其原创权利，并可依 Plugin SDK License 发布。通过 Steam 或其他官方渠道发行，必须接受[插件发布协议](PLUGIN-PUBLISHING-AGREEMENT.zh-CN.md)，包括收益、维护、税务及下架条款。

## 9. 行为与安全

请尊重他人，避免骚扰、违法内容、隐私侵害或误导性声明。未修复漏洞不得公开披露；请通过代码仓库指定的私人联系方式报告。不得包含超出安全复现所必要的攻击 Payload。

## 10. 准据条款与联系

贡献流程及相关 CLA 适用中华人民共和国法律，并由 CIETAC 在北京以中文进行一名仲裁员的仲裁，但保留 CLA 所述不得放弃的消费者保护、小额程序救济及紧急法院救济。英文证据可提交，但被要求翻译的除外。问题可通过 [SuperExplorer GitHub 代码仓库](https://github.com/damody/SuperExplorer)提出。

Copyright © 2025–2026 Damody. All rights reserved.
