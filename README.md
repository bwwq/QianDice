# 千变 Qianbian

Rust 后台、React Web 管理端、Flutter 桌面界面，面向 CoC / DND 的便携跑团骰系。通过 OneBot 11 对接独立 QQ 协议端；数据保存在程序旁，Python 和 Rust 扩展以独立进程接入。

[功能、使用方法与实现图表](docs/features.html)可离线打开、搜索与导出，并包含 Linux OneBot 测试的实际收发报文。

## 启动

Web 版运行 `qianbian.exe`（Windows）或 `./qianbian`（Linux）。默认地址为 `http://127.0.0.1:9610`，管理令牌保存在 `data/config/admin-token.txt`。桌面版运行 UI 入口，关闭窗口不会停止后台。

```text
qianbian --data-dir D:\千变数据
qianbian --listen 127.0.0.1:9611
qianbian backup
qianbian check-backup data/backups/备份名称
qianbian restore data/backups/备份名称 --confirm
```

数据路径默认相对于入口程序，不受终端工作目录影响。恢复前必须停止后台；恢复会回到备份时点，同时保留恢复前的完整备份。一个数据目录只允许一个后台运行。

Linux 发布程序不需要 Rust 或编译器，添加执行权限后即可启动。已准备的独立协议验收工具也无需编译：

```sh
chmod +x dist/qianbian-linux-x86_64 dist/qianbian-protocol-check
python3 scripts/verify-linux-functions.py --backend dist/qianbian-linux-x86_64 --runner dist/qianbian-protocol-check
```

该入口不调用 Cargo，四组测试并发启动隔离后台，收发记录写入 `dist/functional-report/`。Python 插件场景使用环境中已有的 Python 解释器。验收工具由云端构建另行提供，不属于后台启动依赖。

## QQ 连接

在管理界面的「账号与群」配置账号，或停止后台后编辑 `data/config/server.json`。示例不包含实际令牌：

```json
{
  "listen": "127.0.0.1:9610",
  "masters": ["你的QQ号"],
  "prefixes": [".", "。"],
  "accounts": [{
    "id": "main",
    "self_id": "机器人QQ号",
    "mode": "reverse",
    "url": "",
    "token": "请生成至少16字符的独立令牌",
    "enabled": true
  }],
  "blocked_users": [],
  "allowed_groups": [],
  "cooldown_ms": 500
}
```

反向连接使用 `ws://后台地址:9610/onebot/main`，OneBot 端选择 Universal，并使用相同访问令牌。正向连接把 `mode` 改为 `forward`、`url` 设置为协议端的 Universal WebSocket 地址。连接配置及监听地址修改后重启后台；权限、前缀和冷却立即生效。远程访问请使用 HTTPS/WSS 反向代理，默认不对公网监听。

## 跑团

```text
.st new 调查员
.st 力量60 理智65 侦查50
.st lock
.ra 侦查
.rab1 侦查
.rh 1d100
.sc 0/1d6
.en 侦查
.log on 雨夜调查
.log off
.log on
.log end
.log upload 雨夜调查
```

`.help` 查看指令，`.st help` 查看角色操作。群绑定优先于用户默认卡；切换默认卡不会解除群绑定。SC/成长从角色卡读取数值时写回，显式提供数值时仅计算。暗骰只私聊，无法确认送达时不向群公开、不重新投掷。

团录发送目标是当前QQ群。现阶段按要求仅预留群文件上传插件接口，不要求域名、不创建下载站；未接入上传器时，日志保留在本地，管理端可导出完整文件。不同群的同名日志分别保存。

DND 基础提供 `.dnd`、`.adv [加值]`、`.dis [加值]`、`.ri 名称 [加值]` 和 `.init`。CoC 内置房规 0 为 7 版成功等级，房规 1 使用 1—5 大成功。复杂房规可由插件扩展。

## 自定义规则和牌堆

文件保存到 `data/rules/名称.json`、`data/decks/名称.json`，下次指令读取时生效，也可在管理端编辑。规则示例：

```json
{"id":"d20-target","label":"D20目标检定","faces":20,"comparison":"gte","critical":20,"fumble":1}
```

角色使用 `.st temp d20-target`，再用 `.ra 技能 目标值` 检定。牌堆示例：

```json
{"without_replacement":false,"entries":[{"text":"雨夜","weight":2},{"text":"{地点}","weight":1}]}
```

## 插件

目录为 `data/plugins/<插件标识>/<版本>/plugin.json`，管理页输入该相对路径加载。清单、Python 示例和 SDK 在 `examples/python-plugin/`、`sdk/python/`；Rust 示例为 `examples/rust_plugin.rs`。详细协议见 [插件协议](docs/plugin-protocol.md)。

Python 插件需自行指定可用解释器；没有 Python 不影响基础 Rust 功能。插件是可信的本机代码，进程隔离不等于操作系统沙箱。停用保留数据，重新启用恢复使用；接口不兼容或启动失败时显示原因。

## 更新与回退

停止后台及界面，替换程序文件，保留整个 `data/`。不同 Web/UI 版本共用数据。`runtime/` 存储版本资源及旧程序，当前版本不自动清理这些文件，便于离线回退。回退时将保留的发布文件复制回原入口位置，或显式指定原数据目录；不要直接从版本资源子目录启动并期待自动找到原数据。

数据兼容的旧版可直接启动；不兼容的旧版拒绝写入，应使用新版或恢复对应的升级前备份。数据库当前结构版本为 1，尚无跨结构版本的迁移需求。

## 云端构建

本地不需要安装 Flutter/MSVC。CNB `push` 执行 Rust 核心及便携插件契约验证；`api_trigger_release` 生成 Linux Web、Linux UI 和 Windows Web。Windows原生编译和打包在已配置的远程Windows环境执行 `scripts/build-windows.ps1`，再运行 `tests/portable_contract.py` 与 `scripts/check-windows-ui.py`。GitHub Actions保留手动触发作为备用，不再随推送自动重复构建。

Web 使用 React 19.3.0 与 Vite 8.3.2，静态资源嵌入 Rust 程序，使用系统字体，不依赖运行时 CDN、Flutter 或 CanvasKit。`api_trigger_web` 单独验证网页构建、浏览器登录、模拟掷骰和响应式截图。

桌面 Flutter 固定为 3.47.6，平台入口由 `flutter create` 在构建容器中生成。Windows UI 包首次运行释放 DLL 和资源。Linux UI 以 Ubuntu 24.04 为基线，需要 GTK 3、libstdc++、liblzma 和可用的图形会话；Linux Web 无需这些图形库。桌面随包附带 Noto Sans SC 字体及 OFL 许可。

## 当前边界

这是 0.1 开发版本。QQ 真实账号收发需要部署者配置并验证；默认配置不会连接任何账号。反向连接目前要求 Universal。完整验证状态以 [实施记录](docs/implementation-status.md) 和对应提交的云端日志为准。
