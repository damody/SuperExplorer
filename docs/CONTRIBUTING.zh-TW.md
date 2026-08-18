# 參與 SuperExplorer 貢獻

生效日期：2026 年 7 月 31 日  
專案擁有人：Damody  
語言：[English](CONTRIBUTING.md) | **繁體中文** | [简体中文](CONTRIBUTING.zh-CN.md)

> SuperExplorer 是專有、Source Available 軟體，不是開源專案。本指南是商業法律與工作流程範本，正式發布前應由合格律師審閱。

## 1. 準據文本

三語版本應具有相同含義。如有差異，以[簡體中文版本](CONTRIBUTING.zh-CN.md)為準。

## 2. 參與方式

您可以回報問題、提出設計、改善文件、準備核心 Pull Request，或建立獨立插件。檢視原始碼或參與專案不授予重新散布 SuperExplorer 的一般權利；原始碼存取仍適用 [EULA](EULA.zh-TW.md)。

## 3. 核心貢獻與插件分流

對 SuperExplorer 核心、Repository 自有 SDK 素材、文件、測試或專案資產的變更，屬於「核心貢獻」。使用公開介面而獨立維護的擴充功能屬於「插件」，適用 [Plugin SDK License](PLUGIN-SDK-LICENSE.zh-TW.md)。除非 Damody 要求，請勿將整個獨立插件作為核心提交。

## 4. 合併前必須接受 CLA

每項核心貢獻均須事先接受[貢獻者授權協議](CLA.zh-TW.md)。在接受紀錄可歸屬至貢獻者 GitHub 身分前，不得合併 Pull Request。公司貢獻者在必要時須取得雇主授權。

在 CLA 自動簽署功能提供前，請在 Pull Request 說明中加入以下完整聲明：

> 我已閱讀並接受 CLA.zh-TW.md（包括準據文本及爭議條款），並有權依該條款提交本貢獻。

Pull Request 歷史應記錄貢獻者 GitHub 帳戶、Pull Request URL、Commit ID、聲明及接受時間。對重大或公司貢獻，Damody 得要求另行簽署文件。

## 5. 提交 Pull Request 前

1. 搜尋既有 Issue、OpenSpec change 及 Pull Request。
2. 重大行為或架構變更應先討論。
3. 保持變更聚焦，避免無關格式調整或重構。
4. 新增或更新測試及使用者文件。
5. 揭露所有第三方程式碼、生成內容、AI 輔助內容、資產、工具及授權。
6. 確認未包含機密、雇主所有、非法複製或受限制的素材。

## 6. Pull Request 內容

每個 Pull Request 必須說明問題、解法、重要設計選擇、完成的驗證、相容性影響及第三方素材。Commit 必須可審查，且不得包含憑證、個人資料、建置輸出或無關檔案。Damody 得要求修改、拆分工作，或在未接受的情況下關閉提案。

## 7. 審查與接受

提交不要求 Damody 審查、合併、發布、維護、署名或付款。僅在 Damody 合併或以書面確認時才構成接受。Damody 得依 CLA 修改、組合、重新授權、商業化或日後移除已接受的核心貢獻。

## 8. 插件發布

獨立插件作者保有其原創權利，並可依 Plugin SDK License 發布。透過 Steam 或其他官方管道發行，必須接受[插件發布協議](PLUGIN-PUBLISHING-AGREEMENT.zh-TW.md)，包括收益、維護、稅務及下架條款。

## 9. 行為與安全

請尊重他人，避免騷擾、違法內容、隱私侵害或誤導性聲明。未修補漏洞不得公開揭露；請透過 Repository 指定的私人聯絡方式回報。不得包含超出安全重現所必要的攻擊 Payload。

## 10. 準據條款與聯絡

貢獻流程及相關 CLA 適用中華人民共和國法律，並由 CIETAC 在北京以中文進行一名仲裁員的仲裁，但保留 CLA 所述不得放棄的消費者保護、小額程序救濟及緊急法院救濟。英文證據可提交，但被要求翻譯者除外。問題可透過 [SuperExplorer GitHub Repository](https://github.com/damody/SuperExplorer)提出。

Copyright © 2025–2026 Damody. All rights reserved.
