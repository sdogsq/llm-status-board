OpenAI API 卡片升级为组织用量看板，支持 API Platform Admin Key。

- 卡片显示 UTC 今日花费、月初至今花费和组织月度消费上限。若启用了硬上限执行，会显示使用进度。
- 详情新增每日花费、月内累计花费、Token 和请求数趋势，以及模型用量排名。支持本月、7／30／90 天和自定义日期，可按项目及项目 API Key 筛选。
- 花费取自官方 Costs 接口。Token 和请求数来自 Responses／Chat Completions。部分读取权限缺失时，可用的数据仍会显示。

支持 Windows x64、macOS Apple Silicon 和 Intel。可从本 fork 的 v0.2.12 及以上版本通过「设置 → 关于与更新」升级。
