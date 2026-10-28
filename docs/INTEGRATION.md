# 软件连接与任务识别

AI Signal 提供本机事件入口和一个无第三方依赖的 Python 3 适配器。桌面应用必须先启动；浏览器预览没有原生事件服务。此项目没有修改你的 `~/.codex` 配置，没有自动信任 Hook，也没有读取所有 Codex Desktop 会话。


## 连接 Codex

1. 先运行一次 Codex，使本机会话目录存在。
2. 在 AI Signal「连接」中点击 Codex 的「连接」。
3. 应用每 3 秒检查近期有更新的本机会话，自动将有明确开始事件、尚未结束的任务加入列表；优先使用会话标题，缺少标题时使用项目目录名。
4. 断开后停止识别，并移除该来源的临时任务。重新打开应用会记住连接开关，并重新识别仍可读取的运行会话。

此方式只读 `CODEX_HOME` 或 `~/.codex` 中的会话及标题索引，不修改 Codex 配置。它属于本机兼容识别：只扫描最近 24 小时有更新的最多 512 份记录（超过范围时红灯提示监测不完整），冷启动逐行完整恢复近期会话的生命周期，之后只读取新增完整行。单条记录上限为 64 MiB；超大或无法读取的记录会明确提示，继续识别其他记录。不会根据普通对话内容推断运行状态。「任务」和「连接」页提供同步状态与“刷新任务”，用于重新扫描本机记录。

不能保证识别远程、云端、自定义未提供的存储位置、所有子代理或所有 Codex Desktop 会话；记录格式可能改变。已兼容读取 `request_user_input`、`request_user_input_async` 的调用及回答记录：提交被受理不解除红灯，一轮结束不覆盖未答问题；部分回答仅解除对应问题。明确的工具锁屏错误也会保留待处理状态。这仍不能可靠区分所有审批或自动修复，也不是对所有软件界面状态的完整镜像。需要更精确状态时，可补充下面的 Codex Hooks。官方文档明确说明会话 transcript 并非稳定接口：[OpenAI Hooks](https://learn.chatgpt.com/docs/hooks)。

## 连接 Claude Code

1. 先安装并运行一次 Claude Code，使用户配置目录存在。
2. 在「连接」点击 Claude Code 的「连接」。应用把原生状态助手复制到自身数据目录，合并 `~/.claude/settings.json` 中的状态 Hooks，保留已有设置。
3. 在 Claude Code 的受信任工作区继续或开始任务。首次配置、版本或工作区策略可能要求重开会话或确认信任；界面在收到真实任务之前显示「已配置 · 等待任务事件」。
4. 断开只移除带有本应用标记的 Hook，不回滚其他后续设置。

原配置备份位于应用数据目录的 `backups/`，仅本机保存。配置损坏、格式不符、全部 Hooks 被禁用或文件是符号链接时，应用会报错并保留原文件。`CLAUDE_CONFIG_DIR` 可指定自定义配置目录。客户端需要支持官方文档中的命令参数 `args` 形式；较旧版本请先升级 Claude Code。

状态事件：`UserPromptSubmit`、`PreToolUse`、`PostToolUse` 为运行；`PermissionRequest`、权限通知及 `AskUserQuestion` 工具为待处理；`Stop` 为本轮结束；`SessionEnd` 为空闲。普通空闲通知不会误触发红灯。黄灯仍由明确的修复流程上报。Hook 助手无 Python 依赖，失败时退出成功且输出空 JSON，不批准、不拒绝、不阻塞任何工具操作。

此版本验证了配置保留、备份、断开合并与原生助手到模拟接收端的认证投递；当前机器未安装 Claude Code，尚未验证真实 Claude Code 客户端自动事件流。依据：[Claude Code 官方 Hooks 文档](https://code.claude.com/docs/en/hooks)。

## 其他软件

任何能发出本机 HTTP 事件的工具，都可以使用下述通用接口。没有对应接口的软件不会被标成已连接。通用来源任务 ID 避免使用保留前缀 `codex:`、`claude:`、`monitor:`；这两类前缀受软件连接开关控制。

## Codex Hooks 的接入范围与真实含义

2026-10-24 核对了 [OpenAI 官方 Hooks 文档](https://learn.chatgpt.com/docs/hooks)。官方接口通过标准输入传入 JSON，包含 `hook_event_name`、`session_id`、`cwd`；配置支持 `commandWindows`。Hook 需要审阅并信任。`PermissionRequest` 对应审批请求；`Stop` 对应一轮结束。工具结果的格式由工具决定。

本项目采取以下映射，这些是 AI Signal 的显示规则：

| 收到的事件 | 灯与状态 | 解释 |
| --- | --- | --- |
| `UserPromptSubmit`、`PreToolUse`、`PostToolUse` | 绿闪 / `running` | 请求已开始或工具流程继续 |
| `PermissionRequest` | 红常亮 / `attention` | 返回 Codex 查看是否需要人工审批 |
| `Stop` | 绿常亮 / `completed` | **本轮已结束**，不等于整个任务完成或验收成功 |
| `Interrupt`、`SessionEnd` | 全灭 / `idle` | 本轮中断或会话结束 |
| 显式脚本状态 `repairing` | 黄闪 | 由你已知的修复流程或手动模拟发送 |

适配器不根据文本中出现 “error” 判断自动修复；错误结果也可能需要人工处理。当前没有可靠的通用事件能识别所有“AI 正在自修复”和所有“等待用户回答”。本机可读取的 Codex 提问记录已做兼容识别；其他来源仍需明确上报，缺少明确事件时保留最近明确状态，不按静默时间转红。审批请求可能随后自动通过，红灯提示你检查审批界面。子代理结束不会把父任务标成完成；同一会话的工具事件可能交错，以应用最后接收的状态为准。`SessionStart` 不改变状态，避免中途压缩上下文重新触发该事件时错误熄灯。

## 先验证本机链路

在项目根目录打开终端，先运行 AI Signal 桌面应用，再执行：

```sh
python3 integrations/codex_hook.py --status running --task-id integration-check --title "联动测试" --strict
python3 integrations/codex_hook.py --status attention --task-id integration-check --title "联动测试" --strict
python3 integrations/codex_hook.py --status repairing --task-id integration-check --title "联动测试" --strict
python3 integrations/codex_hook.py --status completed --task-id integration-check --title "联动测试" --strict
```

Windows 将 `python3` 换成已安装的 `python`。每次应更新同一条“联动测试”任务。`--strict` 让命令在投递失败时返回非零，便于诊断；正常 Hook 命令不使用它。

先在连接页启用 Codex，再模拟真实的 Hook 输入：

```sh
printf '%s' '{"hook_event_name":"PermissionRequest","session_id":"demo-session","cwd":"/work/demo"}' | python3 integrations/codex_hook.py --event PermissionRequest --strict
```

此步骤只验证 **模拟事件 → 适配器 → 本机应用**，不证明 Codex 已自动发出事件。完成后可在任务列表中删除测试任务。

## 生成可审阅的 Hook 配置

```sh
python3 integrations/generate_hooks.py
```

生成 `integrations/hooks.example.json`，其中命令使用适配器的绝对路径，工作目录变化不影响查找。移动项目后应重新生成。脚本仅写这个示例文件，不会替你安装配置。

Windows 上运行同一生成器会写入 `commandWindows`。如果在 macOS 为另一台 Windows 机器准备配置，可明确传入那台机器的路径：

```sh
python3 integrations/generate_hooks.py --script-windows 'C:\AI Signal\integrations\codex_hook.py' --python-windows python
```

确认路径和 Python 命令适用于目标机器，然后审阅 JSON。若已有 `hooks.json`，合并 `hooks` 下的事件数组，保留原来的 Hook。可放入一个项目的 `.codex/hooks.json`，逐项目试用；也可按你的需要合并到用户级配置。不要直接覆盖现有文件。

安装后在支持这些 Hook 的 Codex 客户端中完成审阅与信任；官方 CLI 提供 `/hooks`。修改 Hook 定义后需要重新审阅。然后用一个真实会话提交请求、触发审批并结束一轮，观察 AI Signal 中是否出现来源为 “Codex Hook” 的任务。只有观察到这一步，才能把**该客户端、该配置范围**标为已验证自动联动。没有实际收到事件时，继续使用手动状态，不应把静默理解成一切正常。

## 本机接口与隐私

应用启动后优先使用 `127.0.0.1:47832`；端口被占用时选择一个可用的本机端口。应用生成当次运行的令牌，并将实际端口与令牌保存到发现文件：

| 平台 | 发现文件 |
| --- | --- |
| macOS | `~/Library/Application Support/io.aisignal.desktop/bridge.json` |
| Windows | `%APPDATA%\io.aisignal.desktop\bridge.json` |
| Linux（源码兼容，未作为本次验收平台） | `${XDG_DATA_HOME:-~/.local/share}/io.aisignal.desktop/bridge.json` |

如果使用了不同的数据目录，可通过 `--bridge /absolute/path/bridge.json` 显式指定。不要把令牌放入共享文档；应用重启后自动重新读取发现文件。

入口为 `POST http://127.0.0.1:<port>/event`，头部为 `Authorization: Bearer <token>` 与 `Content-Type: application/json`。适配器禁用代理和重定向，超时为 0.8 秒，避免指示灯拖慢工作。请求最多 64 KiB；应用为事件重新记录本机接收时间。

```json
{
  "id": "external:my-task",
  "title": "我的任务",
  "status": "repairing",
  "source": "外部脚本",
  "message": "已开始重新运行失败的检查",
  "updatedAt": 1792800000000
}
```

`id` 应在同一任务的各次更新中保持一致；允许的 `status` 为 `attention`、`repairing`、`running`、`completed`、`idle`。Hook 适配器只发送会话标识、目录名和简短状态，不发送提示词、工具参数、工具输出、完整路径或对话正文。外部事件保存在原生进程内存中；启用 Codex 本机连接后，重启时会重新识别可读取的活动会话。手动任务、灯体设置与连接开关会持久化。

Hook 投递失败只在标准错误中给出简短诊断并返回成功，不会批准、拒绝或阻塞 Codex 操作。`Stop` 只输出空 JSON 对象，满足该事件的输出协议。适配器会忽略不支持的事件。

## 验证与限制

```sh
python3 -m unittest discover -s integrations -p 'test_*.py' -v
```

测试覆盖事件映射、隐私字段、非法输入、Windows 路径、HTTP 认证投递、重定向拒绝、投递失败及 `Stop` 输出协议。HTTP 测试使用临时本机模拟接收器，不调用 Codex、不修改任何用户配置。测试结果与原生端实际运行记录见项目验收说明。

本版本未通过安装 Hook 的方式验证正在使用的 Codex Desktop 自动事件流。它无法保证覆盖全部会话、远程机器、浏览器端或退出后的历史任务；应用未运行时事件不会排队重放。同一次应用运行期间，外部任务保留最后接收到的状态，运行／修复状态不会仅因缺少新消息而转红；来源再次发送相同 `id` 的事件时，任务会重新出现。重新启动应用会清空纯事件任务，并重新扫描已启用的 Codex 来源。`Stop` 的绿常亮只表示本轮结束，不代表任务已经验收。开机启动也不在此适配器范围内。
