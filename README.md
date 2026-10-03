# 千变

千变是一款便携的 QQ 跑团骰系，主要支持 CoC 7，兼容 DND 5e 基础指令。提供 Windows、Linux 版本，可通过 React Web 或 Fluent UI 桌面界面管理。

支持掷骰、角色卡、技能检定、理智检定、成长、牌堆和跑团记录，也可以添加自定义规则与 Python、Rust 插件。

## 启动

| 平台 | Web 版 | 桌面版 |
|---|---|---|
| Windows | 运行 `qianbian-windows-web.exe` | 运行 `qianbian-windows-ui.exe` |
| Linux | 运行 `qianbian-linux-x86_64` | 解压桌面包，运行 `qianbian-ui` |

下载对应版本后即可运行。Linux 需要给入口文件添加执行权限；桌面版需要图形会话及 GTK 3、libstdc++、liblzma，支持 Ubuntu 24.04。

Web 管理地址默认为 `http://127.0.0.1:9610`，登录令牌在 `data/config/admin-token.txt`。桌面版会自动连接本机后台。

关闭窗口或浏览器后，骰子仍会运行。需要退出时，在「设置」中停止后台。

## 连接 QQ

千变通过 OneBot 11 连接 QQ，需要先运行支持该协议的 QQ 协议端。

1. 在「账号与群」中添加账号，填写机器人 QQ 号和访问令牌。
2. 使用反向 WebSocket 时，将协议端的 Universal 地址设为 `ws://后台地址:9610/onebot/账号标识`，两端填写相同的访问令牌。
3. 使用正向 WebSocket 时，在千变中填写协议端的 Universal WebSocket 地址。
4. 保存后重启后台，在「账号与群」中查看连接状态。

## 常用指令

默认支持 `.` 和 `。` 前缀。发送 `.help` 查看帮助。

| 指令 | 用途 |
|---|---|
| `.r 1d100` | 掷骰 |
| `.rh 1d100` | 暗骰，结果私聊发送 |
| `.st new 调查员` | 创建角色卡 |
| `.st 力量60 理智65 侦查50` | 设置角色属性 |
| `.st lock` | 将当前角色绑定到群 |
| `.ra 侦查` | 技能检定 |
| `.rab1 侦查` | 带一个奖励骰的检定 |
| `.sc 0/1d6` | 理智检定 |
| `.en 侦查` | 技能成长 |
| `.log on 雨夜调查` | 开始记录跑团 |
| `.log off` / `.log on` | 暂停 / 继续记录 |
| `.log end` | 结束记录 |

DND 可使用 `.dnd` 生成属性，`.adv` / `.dis` 进行优势 / 劣势检定，`.ri 名称 [加值]` 登记先攻，`.init` 查看先攻顺序。

角色卡、规则和牌堆可以在管理界面查看与编辑。团录可以查看并导出为 TXT、HTML、JSON。

更多功能见 [功能与用法图表](docs/features.html)。

## 自定义

自定义规则放在 `data/rules/`，牌堆放在 `data/decks/`，也可以直接在管理界面编辑。保存后即可使用，无需重启。

插件在「插件」页加载、启用或停用，支持热加载。Python 插件需要可用的 Python 解释器。编写插件可参考 [插件协议](docs/plugin-protocol.md)、[Python 示例](examples/python-plugin/) 和 [Rust 示例](examples/rust_plugin.rs)。

## 数据与备份

数据保存在入口程序旁的 `data/` 文件夹。移动整个程序文件夹即可带走数据；Web 版和桌面版可以接着使用同一份数据。

在「设置」中创建完整备份，或按需选择角色与团录、配置、插件及其数据、规则、牌堆和日志文件。可以设置自动备份间隔与保留数量，并在备份后上传到 WebDAV、S3。

更换程序前，退出界面并停止后台，替换程序文件，保留 `data/`。

需要指定其他数据目录时，启动时添加 `--data-dir "数据目录"`。恢复备份前请停止后台；远端备份包先解压，再运行 `qianbian restore "备份目录" --confirm`。自定义备份只恢复所选内容，并保留恢复前的数据备份。
