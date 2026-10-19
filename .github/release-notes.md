## 新增 AiHubMix 支持

- 在「设置 → 添加账号」中选择 **AiHubMix**，填入 API Key 即可查询。
- 展示美元总额度、累计已用额度和余额，自动将接口返回的美分用量换算为美元。
- 余额按 `hard_limit_usd - total_usage / 100` 计算，超额使用时保留负余额。
- 提供凭证教程、连接测试和清晰的计费查询错误提示。

站点地址保持 `https://aihubmix.com`，无需添加 `/v1`。计费接口不提供按日、按模型或 Token 明细；余额趋势随本地刷新记录积累。

## 安装包

- **Windows**：下载 `*-setup.exe` 安装程序。
- **macOS Apple Silicon（M 系列）**：下载 `*_aarch64.dmg`。
- **macOS Intel**：下载 `*_x64.dmg`。
