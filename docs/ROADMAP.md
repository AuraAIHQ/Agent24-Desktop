# Roadmap

> 详见 [PLAN.md 第六节](PLAN.md#六roadmap里程碑) 主里程碑列表。本文档维护当前进度细节。

## 当前状态：M0（Bootstrap）

- [x] 创建 AuraAIHQ/Agent24-Desktop 仓库
- [x] 引入 vendor/xiaoheishu submodule（参考实现）
- [x] 写产品计划（docs/PLAN.md）
- [ ] 设计 CapabilityModule 接口（docs/MODULE_INTERFACE.md）
- [ ] 从 vendor/xiaoheishu/desktop 提取通用 Electron 骨架到主目录
- [ ] AI Layer 接口设计（docs/AI_LAYER.md）

## 下一步（M1 启动条件）

完成 M0 后启动 M1：
1. CapabilityModule 接口定稿
2. 从 xiaoheishu 提取通用代码（保留：Electron shell, IPC, vite/react 配置, AI 模型管理；抽离：xiaohongshu publisher → 独立模块）
3. 第一个 reference 模块（blog 发布）走通端到端
