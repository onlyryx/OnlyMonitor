# Only Monitor · v0.7.0 Alpha

面向 Windows 的本地系统监控工具。Tauri 2 + React + TypeScript + Vite，使用 Tailwind CSS、Recharts、Framer Motion；Rust 通过 sysinfo 采集真实数据。

v0.6 在现有当前进程排行、托盘摘要和可选通知基础上，增加本次运行内的持续高负载回看。事件最多保留 20 条，每条最多记录 3 个确认时的进程线索；只在内存中保留，退出即清空，不上传。进程线索不代表原因；通知默认关闭，是否记录事件与通知开关无关。此版本仍在验证，v0.5 原生验收状态见 [v0.5 验收记录](docs/v0.5-verification.md)。

v0.7 Alpha 增加本机关注、GPU 引擎活动、逻辑磁盘读写速率、手动 Ping 和按需读取系统热区，并更新浅色品牌标识。GPU 引擎是按进程计数器归并的活动时间，不是整卡利用率；系统热区不能认定为 CPU 或 GPU 温度。硬件页面可读取系统目录中 NVIDIA 驱动接口提供的显卡温度；存储页面显示 Windows 报告的物理磁盘状态，并在设备和驱动返回可校验数据时显示部分 SATA 属性。Windows 的状态及故障预测不是完整 S.M.A.R.T. 诊断，正常也不能保证没有故障。CPU 专属温度及 AMD／Intel 显卡温度暂不可用。

未来官网 `monitor.onlyryx.com`；本次没有部署网站或修改 DNS。

## 许可证

Only Monitor 项目代码采用 [Apache-2.0 许可证](LICENSE)，版权所有 © 2026 onlyryx。仓库内第三方代码保留其各自的版权和许可证，例如 [auto-launch 的 MIT 许可证](src-tauri/vendor/auto-launch/LICENSE)。

## 安装与使用

v0.7 Alpha 可按下文从源码运行和构建。未签名安装包已在本机完成静默安装、升级和回退检查；图形安装向导及其他设备仍需验证。具体条件与测试范围见 [v0.7 验证记录](docs/v0.7-verification.md)。v0.6 安装版验收范围见 [v0.6 验收记录](docs/v0.6-verification.md)。

- 首次启动显示仪表盘；默认点击关闭按钮会隐藏到托盘，后台继续采集。
- 左键单击托盘恢复窗口，右键菜单提供显示／隐藏／退出。
- Settings 可将关闭行为改为真正退出；托盘的退出菜单始终退出应用。
- 重复启动恢复已有窗口，避免多份监控进程。
- Settings 支持 1／2／5 秒采样，修改后点击“保存设置”。Network 选择接口后点击“保存接口选择”。
- Activity 按 CPU 与驻留内存分别显示当前排名前 8 的进程；CPU 第一次采样会显示“计算中”。某些受保护或不可访问进程可能不完整。
- Hardware 展示可读取的 GPU 引擎活动；系统热区由用户按需读取，不在后台轮询。Storage 显示可匹配的逻辑磁盘读写速率；首次采样、接口不可用或不支持的卷不伪造零值。
- Settings 中的“持续高负载提醒”默认关闭。开启并保存后，CPU 或内存达到 90% 持续 30 秒时通知一次；该项回落到 80% 以下持续 60 秒后才能再次通知。
- Windows 通知仅对已安装应用正常工作；浏览器预览和开发模式不用于验证通知。通知发送错误会在设置页显示，采集仍会继续。
- 托盘创建失败时不隐藏窗口，关闭按钮改为退出，并在页面说明原因。
- 登录启动默认不启用。在 Settings 点击“启用登录启动”或“关闭登录启动”立即更改当前用户启动项；不提交其他尚未保存的偏好。
- “登录启动时隐藏到托盘”默认勾选，随“保存设置”生效；手动打开及重复启动总会显示窗口。登录隐藏启动在托盘就绪前不显示主窗口，托盘失败则回退显示。
- “已注册”只表示当前 EXE 的登录启动登记存在，Windows 启动应用开关、组策略等仍可能阻止执行；程序不会覆盖系统批准状态。可点击“刷新启动状态”重新查询。
- 移动便携 EXE 或更换安装目录后需在新位置重新启用；同名启动项指向其他副本时报告错误，关闭操作不会删除另一副本的登记。显式启用会更新为当前路径。

## 开发环境

需要 Windows 10/11 x64、Node.js 22.12+、Rust stable MSVC 1.99+、Microsoft C++ 桌面开发工具、Windows SDK 和 WebView2 Runtime。环境准备参考 [Tauri 官方说明](https://v2.tauri.app/start/prerequisites/)。应用本身按普通用户权限设计，不要求管理员权限。

```powershell
npm ci
npm run tauri -- dev
```

单独 `npm run dev` 只提供浏览器预览，明确提示无法读取真实指标，不使用模拟数据。

Rust 工具链可隔离安装在 `.tools/`，不修改系统 PATH。如未安装全局 Rust，可在此项目 PowerShell 中设置：

```powershell
$env:CARGO_HOME = "$PWD/.tools/cargo"
$env:RUSTUP_HOME = "$PWD/.tools/rustup"
$env:PATH = "$PWD/.tools/cargo/bin;$env:PATH"
```

其他机器正常安装 Rust 即可。开发期间因官方下载 TLS 失败，通过 rsproxy.cn 安装隔离工具链；应用运行时不访问该镜像。开发依赖和打包工具下载需要联网，监控采集不需要互联网。

## 检查与构建

```powershell
npm run typecheck
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/vendor/auto-launch/Cargo.toml --lib
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run tauri -- build
```

依赖由 package-lock.json 和 Cargo.lock 锁定；沿用 v0.1 的 Tauri 2.12、React 19、TypeScript 5.9、Vite 7、Tailwind 4、Recharts 3、Framer Motion 12 与 sysinfo 0.37.2。保留 tray-icon 与 single-instance，v0.3 新增官方 autostart 2.5.1，仅由 Rust 调用，没有 Shell／HTTP／通用文件插件。

autostart 的底层 auto-launch 0.5.0 通过 Cargo patch 使用仓库内 MIT 源码。Windows 补丁为路径加引号、不修改 StartupApproved、保留读取错误并限制删除归属；详见 [补丁说明](src-tauri/vendor/auto-launch/PATCH.md)。升级该依赖时需复核补丁行为，不应直接删掉补丁。

## 数据与运行方式

单个 Rust worker 独占采集器，上一轮完成后等待指定间隔再开始下一轮；实际间隔包含系统采集开销。前端仅在可见时串行读取缓存，隐藏后停止界面轮询和 UI 时钟。读取缓存、页面切换与 React 重挂载均不会触发新采集任务。

- CPU：总使用率、品牌、逻辑核心与逐核心活动；首轮等待两次有效采样。
- Activity：按 CPU 与驻留内存分别展示当前排名前 8 的进程；CPU 按整机总算力归一化，新进程等待基线。完整排行只保留当前状态，不进入趋势历史；高负载事件另在内存中保留确认时最多 3 条进程线索。
- 内存：总量、已使用量、使用率及趋势。容量采用 KiB／MiB／GiB 二进制单位。
- 磁盘：总容量、可用空间、使用率，每 30 秒刷新；Windows 卷 GUID 合并重复挂载，GUID 查询失败则按挂载路径去重。不可读／未就绪卷展示可检测错误。
- 网络：逐接口累计收发字节；速率使用单调时钟的实际时间差。首轮、新接口或计数回退时重新建立基线，不显示伪造速度。
- 连接质量：Network 页面手动输入 IPv4 地址或域名后发送 ICMP Echo，展示最近 60 次已完成请求的延迟、未回复率和相邻成功回复的 RTT 波动。域名解析后显示实际探测的 IPv4 地址；IPv6 暂不支持。
- 历史：Rust 内存保留最近 60 秒内最多 60 条采样结果；2／5 秒间隔时点数自然减少。错误条目没有指标值，曲线断线；恢复窗口读取后台已有历史。
- 状态：显示真实采样时间、磁盘独立刷新时间、初始化、失败和部分不可用状态。数据过期阈值为 `max(3.5 秒, 3 × 采样间隔)`。

采样间隔超过 `max(10 秒, 3 × 配置间隔)` 时重新建立 CPU／网络基线，避免跨休眠的平均速率；系统睡眠期间不采集、不主动唤醒电脑。系统时钟明显跳变时重新初始化差分值与不一致的历史时间轴。底层系统调用若卡住，界面可提示过期，但不能保证立即取消 sysinfo 正在执行的系统调用。

## 配置与隐私

设置由 Rust 写入 Tauri 的应用配置目录，通常是 `%APPDATA%\com.onlyryx.monitor\settings.json`。只保存：

```json
{
  "schemaVersion": 3,
  "samplingIntervalSecs": 1,
  "closeBehavior": "hideToTray",
  "selectedNetworkInterface": null,
  "hideOnAutostart": true,
  "highLoadAlertEnabled": false
}
```

保存成功才更新运行设置；损坏、无法读取或不支持版本的配置使用默认值并显示错误，不自动覆盖原文件。用户显式保存时以同目录临时文件替换配置。前端不能指定任意读写路径。

schema 1／2 配置会在内存迁移为 schema 3，保留已有设置，新增高负载通知偏好默认为 false；加载不会改写原文件。显式保存后，旧版本无法读取 schema 3，降级时需恢复升级前配置备份。

登录启动登记单独保存在当前用户 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 的 `Only Monitor` 值，内容为带引号的当前 EXE 路径及固定 `--autostart` 参数；JSON 不保存启用状态。应用不会在启动时自动注册或恢复被删除的启动项。安装版卸载只清理精确匹配本次安装的登记；不删除其他路径副本或修改 Windows 的启动批准状态。

监控指标与历史不落盘，退出程序后清空；不上传监控数据、不提供遥测、不运行任意 Shell 命令、不加载模型或发起 AI 请求。托盘常驻会继续消耗少量本机资源，关闭窗口不等于退出。

## 数据限制

CPU 是逻辑处理器时间占用，与任务管理器基于频率的效用值可能不同；sysinfo 的内存定义也不等于进程内存之和。JSON 的累计字节使用 JavaScript number，超过 2^53 字节存在精度限制。

网络包含虚拟、回环和物理接口，所有接口合计可能重复计数，不代表互联网出口带宽；不会自动主动测速，也不判断整个互联网的连通性。断开但仍枚举到的接口可能显示零速率；极短时间内同名接口替换而未观察到消失，无法可靠辨识。所选接口消失时保留选择并提示不可用。

Ping 默认关闭，仅用户点击开始后解析所填域名并向所选目标发送 ICMP；系统 DNS 解析器和目标会收到相应查询或探测包。应用中的目标和结果仅保留在内存，不上传监控数据。每次请求结束后等待 1 秒再进行下一次，系统 ICMP 超时设为 1 秒；停止、离开页面或隐藏窗口后不再安排新请求，已开始的系统调用可能仍会完成。探测使用 Windows 路由，与流量接口筛选无关；ICMP 可能被过滤，未收到回复不等于网络或应用不可用。RTT 波动是连续成功回复之间往返时间差的绝对值均值，并非所有协议通用的抖动定义。

sysinfo 不提供所有底层刷新失败详情，也可能跳过系统拒绝访问的设备。应用不承诺检测全部 Windows API 失败；GUID 查询失败时无法跨不同路径识别同卷别名。磁盘容量不是每秒刷新。物理盘状态来自 Windows 存储提供程序，并非完整 S.M.A.R.T. 检查，不推测物理盘与盘符的对应关系。

进程驻留内存不会与系统内存用量相加对账；受保护或访问受限的进程可能未出现在排名里。高负载提醒只使用本机 CPU／内存采样；事件回看独立保存少量本次运行中的进程线索。Windows 通知需安装版身份才能完整显示。

## 项目结构

```text
src/
  App.tsx / styles.css    导航、响应式仪表盘、进程排行与事件回看
  charts.tsx             CPU、内存和网络趋势
  SettingsPanel.tsx       设置编辑与保存状态
  StartupPanel.tsx        登录启动登记、查询与恢复刷新
  api.ts / useMonitor.ts  类型化 commands、可见性与状态同步
  history.ts             图表投影、缺口与单位格式
  bindings.ts            Rust 生成的 TypeScript 数据结构
  *.test.ts(x)           前端基础测试
src-tauri/src/
  collector.rs           一次真实系统采样及差分计算
  performance.rs         Windows GPU 引擎及逻辑磁盘性能计数器
  thermal.rs             按需读取通用系统温度热区
  hardware_health.rs     本地读取 NVIDIA 温度及 Windows 物理磁盘状态
  alert.rs               本机高负载通知策略
  events.rs              本次运行的有界高负载事件状态
  runtime.rs             唯一后台 worker、缓存与有界历史
  settings.rs            配置校验与固定路径读写
  desktop.rs             托盘与窗口生命周期
  startup.rs             串行登录启动操作及真实状态
  model.rs               DTO 与类型契约测试
  lib.rs                 commands、单实例与启动／退出
  providers.rs           传感器与 Ollama 预留接口，未启用
```

修改 Rust DTO 后生成类型，再检查前端：

```powershell
$env:UPDATE_BINDINGS = '1'
cargo test --manifest-path src-tauri/Cargo.toml frontend_contract_matches_rust
Remove-Item Env:UPDATE_BINDINGS
npm run typecheck
```

普通 Rust 测试检查已生成类型是否与模型一致。不要直接修改 bindings.ts。

## 后续路线图

v0.4 建立用户选定的视觉风格；v0.5 增加当前进程排行、托盘摘要和可选通知；v0.6 正在增加本次运行内的高负载回看。

- [工作纲领](docs/WORKING-CHARTER.md)：优先级、设计与工程规则、验证证据和发布门槛。
- [长期路线图](docs/ROADMAP.md)：v0.4 至稳定版本的阶段目标、能力边界与进入条件。
- [v0.4 计划](docs/v0.4-plan.md)：实施顺序、逐项验收、交付清单。
- [v0.4 验收记录](docs/v0.4-release-verification.md)：自动检查、原生窗口验证和仍待验证的项目。
- [v0.5 负载感知计划](docs/superpowers/plans/2026-10-27-system-awareness.md)：实施范围、顺序和验收标准。
- [v0.5 验收记录](docs/v0.5-verification.md)：自动检查、安装版通知和仍待验证项目。
- [v0.6 计划](docs/v0.6-plan.md)：方向、边界、实施顺序和验收标准。
- [v0.6 验收记录](docs/v0.6-verification.md)：开发版检查及未完成的原生验收。
- [v0.7 计划](docs/v0.7-plan.md)：品牌、硬件洞察和验证边界。
- [v0.7 验证记录](docs/v0.7-verification.md)：已检查的原生行为和剩余限制。

CPU 专属温度、AMD／Intel 显卡温度、完整 S.M.A.R.T.、显卡总利用率、IPv6 Ping、主题切换、长期历史、Ollama 分析和自动更新尚未实现，不承诺所有设备都可读取系统热区或性能计数器。

显卡温度仅在硬件页面可见时约每 10 秒更新，物理磁盘状态仅在存储页面可见时约每 60 秒更新，均可手动刷新。NVIDIA 驱动接口目前仅从 Windows 系统目录加载；驱动不提供该接口时显示不可用。读取失败不沿用旧结果，不上传采集数据。

对于 Windows 能唯一匹配且允许读取的 NVMe 物理磁盘，存储页还显示设备健康日志中的温度、备用空间、寿命消耗估计和严重警告标志，并可展开查看设备报告的通电次数、通电小时、异常断电次数及介质与数据完整性错误。计数是设备报告的历史累计值，不单独作为当前故障判断；通电小时可能不包含低功耗时间。它只覆盖健康日志中的部分字段，SATA、USB 等设备的完整 S.M.A.R.T. 仍未实现。设备不支持、权限受限或身份不能唯一确认时不会猜测读数。

对于 Windows 能唯一匹配且通过只读存储属性提供温度的 SATA 物理磁盘，存储页显示返回的首个传感器读数；它不一定是综合温度。无法匹配、读取失败或设备不支持时显示不可用。该温度不是 S.M.A.R.T. 健康结论。

对于能唯一匹配且支持 Windows 故障预测接口的 SATA 磁盘，存储页显示设备是否预测故障。未预测故障不代表磁盘一定健康；这是补充信息，不是完整 S.M.A.R.T. 数据。

当故障预测接口的厂商专有数据区符合 SATA 属性记录格式并通过校验时，可展开查看属性编号、当前值、历史最差值和六字节原始值。ID 5、9、12 显示常见释义，其余不认识的 ID 标为厂商定义；不同型号可能采用不同含义，当前值不是百分比，原始值不推断单位。未读取阈值、错误日志或自检结果，因此不能称为完整 S.M.A.R.T. 诊断。唯一标识无效时，只有两侧序列号匹配且各自唯一才会读取；序列号只在本机内存中使用，不显示、不保存或上传。

存储页可按需读取 SATA 设备统计日志，显示设备报告的通电重置次数、通电小时和向主机报告的不可纠正错误次数。仅显示设备标记为支持且有效的原始计数；不支持、读取失败或格式不符的字段不会当作零。错误计数不包括设备后台未向主机报告的错误，不能单独作为故障结论；该功能不自动轮询，也不读取完整 S.M.A.R.T. 阈值、错误或自检日志。

[更新记录](CHANGELOG.md) · [v0.1 验证记录](docs/verification.md) · [v0.2 验证记录](docs/v0.2-verification.md) · [v0.3 设计](docs/superpowers/specs/2026-10-23-v0.3-resident-reliability-design.md)
