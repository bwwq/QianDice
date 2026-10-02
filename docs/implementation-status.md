# 实施与验证记录

当前为 0.1 开发版本。交付以真实构建和运行结果为准，不以页面存在替代功能完成。

## 交付

四个发布文件均在 `dist/`，校验值见 `dist/SHA256SUMS`，来源见 `dist/release-manifest.json`。应用源码对应提交 `9a20ba490908c3943f039c91e81f1d28cb0810df`；后续文档和验收脚本的平台名称修正不改变发布二进制。

| 版本 | 文件 | 字节数 |
|---|---|---:|
| Linux Web | qianbian-linux-x86_64 | 8,165,128 |
| Linux UI | qianbian-linux-ui-x86_64.tar.gz | 24,748,966 |
| Windows Web | qianbian-windows-web.exe | 8,979,968 |
| Windows UI | qianbian-windows-ui.exe | 28,875,264 |

Web 已按用户要求改为 React，桌面保留 Flutter，两者共用 Rust 后台和管理 API。React 静态 JS/CSS 约 285 KB（压缩前），嵌入可执行文件，运行时不依赖 CDN、Flutter Web 或 CanvasKit。

## 验证证据

- [CNB 最终 Linux 桌面验收](https://cnb.cool/wsqlxl/qianbian/-/build/logs/cnb-bfu-1k3vf5dna)：Ubuntu 24.04 构建、Rust 规则测试、便携与插件契约、React 浏览器检查、Xvfb 中的桌面启动及退出全部成功。
- [CNB Web 验证](https://cnb.cool/wsqlxl/qianbian/-/build/logs/cnb-19j-1k3veq4m4)：实际登录、模拟掷骰、宽窄屏截图通过；浏览器异常及控制台错误均为零。
- Windows 在已配置的远程 Windows 10 22H2（10.0.19045）原生编译。日志位于本地 `.scratch/remote-windows-package.log` 和 `.scratch/remote-windows-package-final.log`；出现便携契约 PASS、桌面生命周期 PASS 和 `QIANBIAN_WINDOWS_VALIDATED`。
- 两平台均覆盖中文与空格路径、变更工作目录、单实例、认证、角色作用域、暗骰、插件热加载与停用续用、备份校验与恢复。桌面覆盖关闭窗口后后台继续运行、停止后台后插件无残留；Windows 另覆盖首次自解包。
- 修正了 Windows 发布文件同步句柄权限、MSVC 运行库发现、旧版 PowerShell 的 UTF-8 脚本读取，以及验收脚本占用 SQLite 文件的问题。
- GitHub Actions 保留手动触发作为备用。后续 Windows 默认使用远程构建环境，不重复安装本地工具。

## 已实现范围

- 便携目录、数据版本拒绝、单实例锁、版本资源保留、离线替换更新。
- CoC / DND 基础指令、多角色与群绑定、暗骰、牌堆、基本自定义规则、团录与导出。
- OneBot 11 正向及 Universal 反向 WebSocket、多账号、回执关联、断线重连。
- Python / Rust 进程插件与 SDK、热加载、错误隔离、停用持久化、依赖和接口版本检查。
- 管理 API 认证、SSE、配置表单、隔离模拟聊天、事务存储、完整备份与可恢复的文件替换。
- 模拟插件的宿主存储及定时任务与正式空间分开；配置按提交字段合并，避免不同页面覆盖无关设置。

## 当前边界

- 尚未使用真实 QQ 协议端与账号进行收发验收；真实群负载也未测量。
- 团录群文件上传按用户要求只保留插件接口，不附上传服务、不要求域名。未接入时从管理端导出。
- 部分复杂编辑仍采用高级 JSON；角色创建和群内操作主要使用指令。
- 插件按可信代码运行。自行访问的外部文件及服务不受宿主隔离或一致性备份保证。
- 数据库目前只有结构版本 1；未来结构升级必须新增迁移和升级前备份。现有版本拒绝写入更高结构版本的数据。
- 核心升级需要短暂停机；插件和内容可热加载。旧版资源不自动清理，回退前应核对数据兼容性。

## 内存记录

无 QQ 账号、启用五个内置插件；按进程 RSS / Working Set 记录，共享页会重复计数。这些是特定环境的样本，不是占用承诺。

| 场景 | 核心 | 五个插件合计 | 桌面 UI |
|---|---:|---:|---:|
| CNB Linux Web，连接浏览器并完成一次隔离 1d1 试掷 | 8.3 MiB | 21.6 MiB | 不运行 |
| CNB Linux 桌面，Xvfb 虚拟显示 | 8.7 MiB | 20.1 MiB | 335.4 MiB |
| 远程 Windows 10 桌面，空闲 | 14.6 MiB | 31.2 MiB | 67.4 MiB |

浏览器和 QQ 协议端不计入以上数字。Linux 虚拟显示/软件渲染不能代替实际 GPU 桌面基准。Windows 初版测量脚本曾把平台名称写死为 windows-2022，实际主机已核实为 Windows 10 22H2；该标签现已改为自动读取，内存原始数值未改动。
