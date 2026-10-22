# cpr-configPlugin

将「模型路由分组」和「模型基础配置」合并的 CPR 管理插件，并包含通用 Windows Codex 配置 BAT。**不修改 CPR 本体、前端、iframe 权限或 Caddy 配置。**

当前插件版本 **1.0.4**，面向 CPR **3.29.7** 构建并验证；使用插件进程协议 v5、management 合同 v2。声明范围 `>=3.29.7, <4.0.0`，后续宿主仍需核对协议兼容性。

## 功能

- 「上游账号」页：直接列出 CPR 原生账号的真实身份——OAuth 显示登录邮箱与套餐，API Key 显示实际上游地址（仅协议、主机和端口，不显示密钥或路径），以及 CPR 账号分组、状态、最近使用和当前规则；可搜索、筛选并直达该账号的模型规则。插件只投影允许清单内的非敏感字段，不转发令牌。
- 两个保留原布局的管理页面：账号原生白/黑名单及可复用分组；基础模型 JSON 与 Base/Codex 导出。
- 基础 JSON 只维护 display_name、reasoning_levels、context_window、max_output_tokens、input_modalities，保持排序。
- Codex 导出与 BAT 输出相同排版，每个模型附加一次 ultra（Codex 客户端选项，不声明上游原生支持）。
- 通用 BAT 运行时输入 URL/API Key，验证后备份、覆盖 models.json、合并 config.toml。默认 model/review_model 为 gpt-6.1-sol，默认思考 high，Provider Codex Proxy RS，WS=true，空闲等待900000ms。成功后任意键退出。
- BAT 不含用户密钥或服务器模型数据；它在 `downloads/codex-setup.bat` 资源中，属于插件包的一部分。

## 下载 BAT：遵守原生隔离

CPR 的插件 iframe 禁止直接下载。导出JSON选择Codex后，「选中全部」被替换为「下载 BAT」；点击会打开说明窗口，提供 **Release 下载链接及复制按钮**。请在浏览器地址栏打开链接下载。

不会伪装成一键下载，不使用弹窗逃逸，不添加 allow-downloads。浏览器不允许剪贴板时会选中链接，按Ctrl+C即可。未保存草稿不会进入通用BAT。

## 身份与迁移

展示名「模型配置管理」；底层身份保留 `donzy.model-routing-groups`，保留旧路由实例及 `routing-groups` schema1私有状态。切换版本不会自动应用账号规则；基础文件继续使用 `/app/.runtime/data/model-capabilities/models.common.json`。

先备份并通过原生插件版本切换功能升级路由实例，核对分组、账号规则、模型JSON，再停用旧的 `donzy.model-capabilities` 实例。建议保留旧实例/包作回滚，不删除数据库或覆盖备份。首次安装打开「模型基础配置」会提示文件尚未创建；点击「初始化空配置」后，插件以CPR运行用户创建目录（0700）及空对象文件（0600），然后可导入JSON或新增模型。读取页面不写文件，初始化以原子方式拒绝覆盖。存在旧文件、备份、锁或其他历史记录时，不视为首次安装，请检查路径/权限或从备份恢复；不要覆盖已有文件。

## 配置读取服务

普通 API Key 不能访问管理接口。因此仓库保留独立的只读 `codex-setup` 配置服务：GET `/api/codex/setup`，用本机 CPR `/v1/models` 验证 Bearer Key，读取插件保存文件并返回 Codex 目录及客户端参数。不执行真实模型生成，不返回凭据，不写源文件。

服务只监听回环，不属于数据面路由插件。已有该服务的部署可只升级服务二进制，保留原配置和原代理路由；BAT已嵌入服务二进制，与插件包/Release同源。**本仓库不自动修改 CPR、代理或 Caddy**。新服务器如没有该独立接口，BAT会明确报404；需要由管理员按环境单独部署服务及对应入口，这不是仅安装管理插件就能自动提供的接口。

默认路径和端口见 `codex-setup/server/main.go`，可通过CLI参数更改；不要公开暴露回环端口或管理员接口。策略文件 `client-policy.json` 不包含Key；模型目录不是访问授权，实际权限仍由CPR执行。

## 配置服务的 UID 隔离（重要）

独立配置服务必须使用**与 CPR 不同的系统 UID**。不要为了读取0600文件而复用CPR的UID（例如10001），也不要仅因为当前健康检查通过就认为部署完成。某些节点的UID专属防火墙/启动校验会拒绝其他服务共享CPR身份，问题可能到下一次CPR自更新或重启时才暴露。

如果源文件保持私有权限，推荐让受限的root同步任务只读取固定的模型元数据路径，原子发布到独立目录，再让配置服务以独立UID只读副本。同步需要监听源目录以支持插件的原子替换，并设定时兜底；源文件读取或校验失败时应让下载失败，而不是默默提供过期副本。不要放宽源数据目录权限，不要绕过CPR的启动守护。

仓库的systemd unit是参考样例，不能直接照搬用户名、UID和路径到另一台服务器。必须核对独立账户身份、文件访问方案及该节点的启动前校验。发生UID冲突时先停用辅助服务，保障CPR恢复；禁止重新启用共享CPR UID的旧unit。

## 构建（Linux ARM64 发布平台）

需要Python3、Node24、Rust1.97+、Go1.24+、已安装Rust aarch64-unknown-linux-musl target：

```sh
rustup target add aarch64-unknown-linux-musl
python Scripts/build.py
```

产物在dist：受管插件tar.gz、通用BAT、独立服务LinuxARM64二进制。打包脚本生成标准 protocol 5 清单和每个文件的SHA256，不包含任何配置数据、凭据或部署日志。Windows构建需Go可执行文件，或者先单独交叉构建服务，再用 `--skip-go`。

## 测试

```sh
python Scripts/build-bat.py
npm --prefix frontend ci --ignore-scripts
npm --prefix frontend run build
cargo test --manifest-path backend/Cargo.toml --locked
cargo build --manifest-path backend/Cargo.toml --locked
python Scripts/ipc-routing.py
python Scripts/ipc-capabilities.py
node --experimental-strip-types --test Scripts/export.test.mts
node --experimental-strip-types --test Scripts/routing-presentation.test.mts
cd codex-setup/server && go test -race ./...
```

IPC测试启动真实插件子进程，但宿主账号/状态均为虚构测试数据，不连接生产数据库。Windows BAT及部署验收结果在Release说明中给出；模型目录可加载不代表所有上游支持ultra、生图或远程压缩。

## 许可

Apache-2.0。第三方说明见NOTICE；Nett 0.15.0 MIT许可单独保留。gateway-plugin-sdk 固定取自官方 CPR v3.29.7；保留源码及许可，仅将 Cargo workspace 元数据展开以支持独立构建。
