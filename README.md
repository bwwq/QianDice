# 千变 Qianbian

Rust 后台、Flutter 管理界面，面向 CoC / DND 的便携跑团骰系。通过 OneBot 11 对接独立 QQ 协议端；数据保存在程序旁，Python 和 Rust 扩展以独立进程接入。

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

停止后台及界面，替换程序文件，保留整个 `data/`。不同 Web/UI 版本共用数据。`runtime/` 存储版本资源及旧程序，当前版本不自动清理这些文件，便于离线回退。

数据兼容的旧版可直接启动；不兼容的旧版拒绝写入，应使用新版或恢复对应的升级前备份。数据库当前结构版本为 1，尚无跨结构版本的迁移需求。

## 云端构建

本地不需要安装 Flutter/MSVC。CNB `push` 执行 Rust 核心及便携插件契约验证；`api_trigger_release` 生成 Linux Web、Linux UI 和 Windows Web。确认 CNB 通过后，将代码同步至授权的 GitHub 仓库，GitHub Actions 构建 Windows UI 单文件包。

Flutter 固定为 3.47.6。平台入口由该版本的 `flutter create` 在构建容器中生成；Web 资源、字体和渲染器打包进 Rust 程序，不依赖运行时 CDN。Windows UI 包首次运行释放 DLL 和资源，而非无运行资源的纯单文件程序。

## 当前边界

这是 0.1 开发版本。QQ 真实账号收发需要部署者配置并验证；默认配置不会连接任何账号。反向连接目前要求 Universal。完整验证状态以 [实施记录](docs/implementation-status.md) 和对应提交的云端日志为准。
