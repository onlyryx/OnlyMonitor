# v0.1.0 Alpha 验证记录

验证日期：2026-10-22（Asia/Shanghai）。项目初始目录为空，没有已有代码或配置被覆盖。

## 环境与构建

- Windows x64，系统报告版本 `10.0.26100`。
- Node.js 24.18.0、npm 11.16.0。
- 隔离安装 Rust stable 1.99.1、rustfmt、Clippy，未修改系统 PATH。
- 复用本机 MSVC 14.28 与 Windows SDK；虽然 Visual Studio 安装器报告旧安装不完整，实际原生编译、链接及打包通过。
- 产品文件属性已检查：名称 Only Monitor，产品版本／文件版本均为 0.1.0。

| 检查 | 最终结果 |
| --- | --- |
| `npm run typecheck` | 通过，最终生产构建中执行 |
| `npm test` | 通过，2 个测试文件、6 项测试 |
| `npm run build` | 通过 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | 通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 通过，5 项 Rust 测试，二进制与文档测试无失败 |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | 通过 |
| `npm run tauri -- build --debug --no-bundle` | 通过，生成 Windows 调试可执行文件 |
| `npm run tauri -- build` | 通过，最终 Release 可执行文件与 NSIS 安装程序已生成 |
| `npm audit --omit=dev` | 当前锁定生产依赖未报告已知漏洞；不代表完整安全审计 |

Vite 保留一条非阻断提示：单个压缩 JavaScript 包约 730.66 kB（gzip 224.74 kB），超过默认 500 kB 提示阈值。未通过提高阈值隐藏提示。该提示不影响本次构建通过。

## 基础测试覆盖

前端验证历史最多 60 点且不修改旧数组、采样中断后剔除超过 60 秒的点、单位格式、慢请求不重叠及卸载后不再刷新、采集失败后的恢复、请求完成后等待一秒再采样（避免计时抖动导致命中后端缓存）。

Rust 验证速率首轮／重置／实际时间差、磁盘重复挂载去重及零容量不可读状态、首轮 CPU 与网络初始化、重复请求缓存、两次真实系统采样及磁盘缓存、生成 TypeScript 类型与 Rust DTO 的一致性。

## 实机界面检查

启动最终 Release 可执行文件，通过本机 WebView2 调试连接观察实际 UI 与 Tauri command 返回值；没有注入模拟系统数据。

- 成功读取 8 个逻辑核心、17,095,974,912 字节系统内存、5 个卷、2 个网络接口；本次样本无采集警告。数值是本机观测，不是固定演示数据。
- 观察 UI 采样时间连续变化，记录到的相邻更新间隔为 1007–1032 ms；CPU SVG 曲线路径包含多个真实历史点。
- Overview、Network、Storage、Settings 导航均实际点击并验证对应内容出现。
- 检查 1320、900、760、480 px 视口，均无横向溢出；480 px 为 WebView 视口仿真，应用窗口自身最小宽度配置为 760 px。
- 检查期间未捕获到前端 pageerror 或 console error。
- 已查看截图确认主界面及窄视口布局。Overview 显示前 3 个卷，Storage 展示全部卷。

观测结果：[smoke-result.json](smoke-result.json)。本机验收截图未纳入公开仓库。这是开发阶段的本机验证产物，不是应用持续保存监控数据的功能。

测试使用的远程调试参数只在开发进程启动时临时传入，没有写入应用配置或启动代码；检查结束后关闭该测试进程。

## 产物

- `src-tauri/target/release/only-monitor.exe`
- `src-tauri/target/release/bundle/nsis/Only Monitor_0.1.0_x64-setup.exe`（2,113,096 字节）

最终文件 SHA-256：

```text
only-monitor.exe
6D261AF594A4B417100CC1CA98A4C1F9766C9A366BF7EC79668C8AC112BFC4DE

Only Monitor_0.1.0_x64-setup.exe
142B812B20D70D9038200D2D20F38BEA910E4C3D6105A074677644327907E7D9
```

## 未验证与未实现

未执行安装程序的实际安装、卸载、升级测试；未做代码签名、SmartScreen 信任认证、多个 Windows 版本／硬件设备矩阵、长期压力运行、真实拔插设备与受限账户测试。磁盘不可访问／重复挂载与网络计数回退的边界通过逻辑测试验证，没有人为改变本机卷挂载或接口配置。

sysinfo 不暴露所有底层刷新失败详情，设备也可能被底层枚举跳过；应用仅能明确呈现可检测的失败。完整限制见 README。

温度、GPU、Ping、磁盘 I/O／健康、自定义设置、托盘、开机启动和 Ollama 分析均未实现，界面标记 Coming Soon。没有网站部署、DNS 修改、监控数据上传、任意 Shell 执行或 AI 请求功能。
