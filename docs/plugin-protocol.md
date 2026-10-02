# 千变插件协议 v1

插件为独立程序。stdin/stdout 传输一行一个 UTF-8 JSON-RPC 2.0 消息，单条上限 4 MiB，日志只能写 stderr。宿主一次向同一插件派发一个请求；插件可以在返回之前反向调用宿主。

## 生命周期

启动后先调用 `initialize({api:1, preflight:true})`，必须返回 `{api:1}`。预检阶段不能调用宿主能力。业务入口为 `command`，参数包括 `context`、`command`、当前账号的 `world` 快照及可用牌堆、规则。

返回 `{public:"群内或当前会话回复", private:"可选的私聊回复"}`。外部插件默认不替换世界快照；需要更新角色数据时可返回完整 `world`，由宿主按原始版本号提交。插件按可信代码处理，world只包含该账号范围。

一般业务错误返回 JSON-RPC error，不会因此停用进程。协议损坏、进程退出或25秒执行超时会停用，不自动重放。热更先预检新版，再等待旧请求排空，切换后结束旧进程。停用不删除插件数据。

## 已提供的宿主调用

| 方法 | 能力 | 参数和结果 |
|---|---|---|
| `dice.roll` | `dice` | `{expression}` → `{expression,total,detail}` |
| `storage.get` | `storage` | `{key}` → `{value,revision}` |
| `storage.put` | `storage` | `{key,value,revision}` → `{revision}`，版本不一致失败 |
| `config.get` | 无额外能力 | `{}` → `{value,revision}`，管理页维护的插件配置 |
| `message.reply` | `reply` | `{text,private:false}` → `{queued:true}`，当前调用完成后由宿主发送 |
| `schedule.put` | `schedule` | `{id,delay_seconds,every_seconds:0,payload}`，按当前会话创建任务 |
| `schedule.cancel` | `schedule` | `{id}`，取消当前插件自己的任务 |

存储限制在当前插件命名空间。命令回复由返回值交给宿主发送，暗骰私聊失败不回退到群。清单 `events:["message"]` 可订阅消息，入口为 `event`；定时任务也调用 `event`，参数含 `event:"timer"`、原会话 `context`、`id` 和 `payload`。Python/Rust SDK将这两种入口交给同一个处理函数，以参数的 `event` 区分。

任务执行前持久标记为running；崩溃或失败的任务不自动重放。重复任务成功后才计算下次时间，停用插件时不派发任务。message.reply只允许当前会话或当前用户，不能伪造其他会话上下文。模拟会话的定时任务不会向QQ发送结果，宿主管理的插件存储和定时任务也与正式数据分开。可信插件自行访问的外部文件或服务不受宿主隔离约束。

## 清单和部署

`plugin.json` 字段：`id`、`version`、`api`、`entry`、`args`、`commands`、`rules`、`dependencies`、`capabilities`、`config_schema`。程序路径支持绝对路径，相对路径基于插件版本目录。Python entry 建议指定虚拟环境解释器绝对路径；不要把字符串拼成 shell 命令。

Python 示例：把 `examples/python-plugin` 及 `sdk/python/qianbian.py` 放入 `data/plugins/python-example/1.0.0/`，修改解释器路径，在管理页加载 `python-example/1.0.0/plugin.json`。

Rust 示例：云端运行 `cargo build --release --example rust_plugin`，把产物和 `examples/rust-plugin.json` 放入对应版本目录，清单改名 `plugin.json`；Windows entry 需使用 `.exe` 名称。

## 群文件上传扩展点

按用户要求暂不搭建下载服务，也不要求公网域名。`.log end` 和 `.log upload [名称]` 会寻找声明 `group_file_upload` 能力的插件，调用 `file.upload`：

```json
{"event":"file.upload","file":{"account":"main","group":"群号","path":"宿主生成的临时TXT绝对路径","name":"团录.txt","content_type":"text/plain; charset=utf-8"}}
```

插件通过未来配置的协议端上传方式发送文件，只有确认群文件发送成功才返回 `{"confirmed":true}`。当前不随附实际上传器；未接入时日志继续保存在数据库并可从管理端导出。文件只在本次调用期间有效，接口不包含外链。失败不会自动重发，不把团录正文刷到群里。
