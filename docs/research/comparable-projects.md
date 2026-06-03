# 同类项目调研

本文件保存 TheBrain 的同类项目参考。它不替代 README，也不代表 TheBrain 要照搬这些项目。

## 项目清单

| 项目 | 定位 | 链接 |
| --- | --- | --- |
| Obsidian | 本地 Vault + Markdown + 插件生态 | https://obsidian.md |
| Logseq | 隐私优先的 outliner / knowledge graph | https://github.com/logseq/logseq |
| TriliumNext Notes | 层级笔记、属性、脚本、自托管同步 | https://github.com/TriliumNext/Trilium |
| AFFiNE | Notion + Miro 式 block/canvas/workspace | https://github.com/toeverything/AFFiNE |
| Anytype | local-first、P2P、对象/类型模型 | https://github.com/anyproto/anytype-ts |
| Dendron | 面向开发者的 Markdown PKM | https://github.com/dendronhq/dendron |
| Foam | 基于 VS Code/GitHub 的 Markdown knowledge graph | https://github.com/foambubble/foam |
| Joplin | 离线优先 Markdown 笔记、TODO、同步、Web Clipper | https://github.com/laurent22/joplin |
| Khoj | AI second brain，本地/云 LLM 和文档问答 | https://github.com/khoj-ai/khoj |
| Open WebUI | 自托管 AI 平台，RAG、插件、模型管理 | https://github.com/open-webui/open-webui |
| AnythingLLM | Chat with your docs，一体化文档问答产品 | https://github.com/Mintplex-Labs/anything-llm |
| DocsGPT | 私有 AI 搜索和文档问答平台 | https://github.com/arc53/DocsGPT |
| LlamaIndex | RAG/agent 框架和 pipeline 参考 | https://github.com/run-llama/llama_index |

## 值得借鉴

### Obsidian

Obsidian 最重要的参考点是数据边界：Vault 是用户文件，配置和插件状态另放内部目录。TheBrain 应保持同样原则：Vault 是用户资产，`.thebrain/` 是索引、日志、Trace、队列和内部状态。

### Logseq

Logseq 在 README 和文档中会明确标注 beta、alpha 和数据风险。TheBrain 也应明确标注真实能力、实验能力、placeholder 和后续目标，尤其是 RAG、embedding、多格式解析和自动整理。

### Dendron

Dendron 的渐进式结构很适合 TheBrain。用户可以先自由写 Markdown，再逐步增加目录、标签、schema、模板和引用。TheBrain 的 AI 整理应是“建议和执行结构”，不是破坏用户自由组织。

### Joplin

Joplin 把 offline-first、import/export、sync、E2EE、clipper 分得很清楚。TheBrain 应同样把本地优先、AI provider、同步、联网能力、导入类型分层描述。

### Khoj / AnythingLLM / DocsGPT / Open WebUI

这些项目说明 RAG 产品需要清楚拆分：

- 支持文件类型。
- 模型来源。
- 索引和向量库。
- 引用。
- Agent 和工具。
- 部署方式。

TheBrain 当前只应承诺本地 md/txt RAG + citation + Trace，把 PDF/DOCX/PPTX/URL 和真实 embedding 放到 Roadmap。

### LlamaIndex

LlamaIndex 把 RAG 拆成 ingestion、indexing、retrieval、query、persistence、integrations。TheBrain 的架构文档也应按 pipeline 写，而不是只写 UI 功能。

## 不应照搬

- 不照搬 AFFiNE / Anytype 的完整 block/object OS，否则会偏离 Markdown-first。
- 不照搬 Open WebUI / DocsGPT 的企业多用户、权限、Kubernetes、SSO。
- 不照搬 AnythingLLM 的全模型、全向量库、全格式路线。
- 不照搬 Logseq DB 模式迁移，避免 Markdown 与 SQLite 的 source of truth 变得模糊。
- 不照搬 Trilium 的任意深层功能堆叠，避免进一步扩大范围。

## 对 TheBrain 的结论

TheBrain 应优先做成：

1. 本地 Markdown Vault。
2. 收集箱自动整理器。
3. 带引用的本地文件问答系统。
4. TODO、日程、人名档案和个人关系管理的个人第二大脑。

TheBrain 不应过早变成：

- 企业 RAG 平台。
- 多用户协作系统。
- 完整对象数据库。
- 全格式全模型平台。
