# 千变 Qianbian

千变是面向 CoC / DND 的便携跑团骰系，提供 React Web 和 Fluent UI 桌面界面。通过 OneBot 11 对接独立 QQ 协议端，数据保存在程序旁，支持 Python 与 Rust 插件。

[功能与用法图表](docs/features.html)可离线查看、搜索与导出。

## 启动

| 平台 | Web | Fluent 桌面 |
|---|---|---|
| Windows | qianbian-windows-web.exe | qianbian-windows-ui.exe |
| Linux | qianbian-linux-x86_64 | 解压桌面包后运行 qianbian-ui |

直接运行对应入口，无需编译。Linux 可执行文件需先添加执行权限。默认 Web 地址为 `http://127.0.0.1:9610`，管理令牌位于 `data/config/admin-token.txt`。关闭桌面窗口或浏览器页面不会停止后台；停止后台使用设置页的单独操作。

Linux 桌面需要 GTK 3、libstdc++、liblzma 及图形会话，运行基线为 Ubuntu 24.04。Linux Web 无需图形环境。Windows 桌面包首次启动会自动释放随包资源。

```text
qianbian --data-dir D:\千变数据
qianbian --listen 127.0.0.1:9611
qianbian backup
qianbian check-backup data/backups/备份名称
qianbian restore data/backups/备份名称 --confirm
```

数据路径默认相对于入口程序，不受终端工作目录影响。恢复前必须停止后台；恢复会回到备份时点，同时保留恢复前的完整备份。一个数据目录只允许一个后台运行。

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

## 更换版本

停止后台及界面，替换程序文件，保留整个 `data/`。不同 Web/UI 版本共用数据。`runtime/` 存储版本资源及旧程序，当前版本不自动清理这些文件，便于离线回退。回退时将保留的发布文件复制回原入口位置，或显式指定原数据目录；不要直接从版本资源子目录启动并期待自动找到原数据。

数据兼容的旧版可直接启动；不兼容的旧版拒绝写入，应使用新版或恢复对应的升级前备份。
