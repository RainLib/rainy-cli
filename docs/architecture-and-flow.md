# Rainy CLI 当前架构与流程

本文描述当前仓库已经实现的架构、主流程和已知不足。它面向维护者、Agent 和后续实现者；愿景级设计仍以根目录的 `Rainy_CLI_最终形态程序设计与详细开发文档.md` 为参考。

## 项目定位

Rainy CLI 是一个用 Rust 实现的软件能力编排 CLI。它把能力接入过程拆成稳定的命令和协议，让开发者、Agent、Backstage、CI 可以用同一套流程完成：

```text
Plan -> Diff -> Policy -> Apply -> Doctor -> Verify -> Evidence
```

当前开源仓库提供的是 Rainy 的核心实现和一组 community packs。企业内部 starter、审批系统、密钥系统、权限平台等能力不在当前仓库内，需要通过私有 capability pack、plugin 或集成层接入。

## 当前仓库分层

- `crates/rainy-cli`: 当前唯一 Rust crate，包含 CLI 命令树、配置、registry、plan、action、policy、patch、doctor、verify、evidence、plugin、schema、conformance、audit、self-update 等模块。
- `community-packs`: 内置开源能力包，覆盖 Spring Boot、Next.js、Docker Compose、PostgreSQL、Redis、MinIO、OIDC/Keycloak、OpenAPI、Dev Container、GitHub Actions、OpenTelemetry、Helm draft。
- `schemas`: Rainy 项目、能力包、计划、变更集、报告、插件、审计、自更新等 JSON Schema。
- `integrations/mcp`: MCP stdio wrapper 示例，供 Agent 以 JSON-RPC 方式调用 Rainy CLI。
- `integrations/backstage`: Backstage scaffolder action 和 template 示例。
- `scripts`: GitHub Release 安装脚本，按操作系统和 CPU 架构下载对应 CLI 包。
- `.github/workflows`: CI 和 release workflow。CI 负责格式、测试、clippy、schema、MCP wrapper、安装脚本和 smoke；release 负责发版前验证、多平台构建和发布 assets。

## 核心模块职责

`main.rs` 是命令分发和全局横切逻辑入口。它解析 CLI 参数，执行自更新提示，按命令路由到各模块，并在成功或失败后写 audit log。

`cli.rs` 定义命令树和参数结构。当前命令包括 `new/init/add/apply/capability/pack/doctor/verify/evidence/plugin/agent/skill/conformance/schema/self`，未知顶层命令会进入 plugin external forwarding。

`config.rs` 负责 `rainy.yaml` 和 `capability.lock` 的读写。`rainy.yaml` 描述项目、路径、registry source、policy、verify 配置；`capability.lock` 记录已安装能力、provider、版本、artifacts 和 skills。

`registry.rs` 负责加载 capability pack 和 capability definition。当前支持内置 community packs、本地 source、git cache source 和 HTTP registry source，并提供 pack install/update/sign/verify、capability list/explain/graph。内置 community packs 与 JSON schemas 会编译进可执行文件，独立安装后按版本提取到只读运行缓存，不依赖构建机或源码仓库路径。

`actions.rs` 负责把 capability action 转换成 `ExecutionPlan` 和 `ChangeSet`。它处理依赖检查、provider 选择、模板变量、内置 action 执行、plan file 重放、upgrade/remove，并生成 capability lock 更新。

`patch.rs` 负责文件级 diff 和 apply。它生成 create/modify/delete change，支持 no-op 判断，apply 时逐个写入，失败后按已应用变更回滚。

`policy.rs` 是 apply 前的安全门。它合并系统、用户、工作区和 capability policy，检查敏感路径、allow/deny edit、需要审批的 action，以及危险命令。

`doctor.rs`、`verify.rs`、`evidence.rs` 分别负责项目健康检查、能力验证和证据产出。Evidence 会汇总当前配置、lock、doctor、verify 和变更信息，并输出 Markdown/JSON。

`plugin.rs` 负责外部扩展。当前支持安装插件、列出/查看插件、调用 plugin action、转发 external command、HTTP adapter 和 Wasm action plugin。插件返回的变更仍要经过权限校验和主 CLI policy gate。

`schema.rs` 和 `conformance.rs` 负责 schema list/validate 以及 pack/plugin conformance 检查。Schema 使用标准 Draft 2020-12 validator，并将仓库内相对引用装配为本地 definitions。

`update.rs` 负责版本检查、自更新和跳过版本。它通过 GitHub latest release 获取版本信息，并调用 release 中的安装脚本完成更新。

## 主流程

### 1. 初始化项目

`rainy new` 和 `rainy init app` 由 `init.rs` 生成 Golden Path 项目骨架，包括：

- `rainy.yaml`
- `capability.lock`
- `AGENTS.md`
- backend/frontend 基础目录
- compose、CI、openapi、charts、evidence 等目录或文件

`new/init app` 默认会写入文件，也支持 `--dry-run` 预览。

### 2. 发现能力

能力发现由 `registry.rs` 完成：

```text
rainy capability list
rainy capability explain <id>
rainy capability graph
```

Registry 会先读取项目配置，再加载配置中的 source 和内置 community packs，最终按 capability id 提供查询。

### 3. 计划能力变更

能力接入通常从 dry-run 开始：

```text
rainy add capability <id> --dry-run
```

流程是：

```text
load config/lock
load registry
resolve capability
check dependencies
resolve provider
render inputs/templates
run built-in action planners
append capability.lock change
return ExecutionPlan + ChangeSet + rendered diff
```

如果指定 `--output-plan`，CLI 会把 plan 写成 JSON，后续可通过 `rainy apply --plan` 重放。

### 4. 应用能力变更

写文件必须显式 `--apply`：

```text
rainy add capability <id> --apply
rainy apply --plan plans/<id>.json --apply
```

apply 流程是：

```text
plan change
check plan policy
check change policy
apply changes transactionally
write audit log
print human or JSON output
```

如果任意文件写入失败，`patch.rs` 会回滚本次已经写入的文件，避免项目处于半应用状态。

### 5. 验证与证据

`rainy doctor` 检查项目基础文件、lock、默认开发 secret、能力 artifacts 和 capability doctor checks。

`rainy verify --profile local` 运行项目配置和 capability validations，适合开发机使用；对 Maven、Node、Docker 等外部工具缺失可给出 warning。`rainy verify --profile ci` 是严格门禁，未知步骤、registry 加载错误、缺失验证工具链都会失败。Golden Path 生成的 GitHub Actions 会准备 Java/Maven/Node/pnpm、安装前端依赖、安装 Rainy CLI，再运行严格 `ci` 验证。

`rainy evidence generate` 会生成 evidence 文件，把配置、已安装能力、doctor、verify 和变更摘要整理成可进入 PR 或审计链路的材料。

### 6. Pack 和 Plugin 扩展

Pack 是能力定义的主要扩展方式。维护者可以通过 `rainy pack install/update/sign/verify` 管理本地、git 或 HTTP source，并用 `rainy conformance check` 检查协议一致性。HTTP registry 必须为每个文件声明 SHA-256；下载先进入临时缓存，完整校验 Pack 身份和内容后再原子替换，失败时保留旧缓存。`capability.lock` 继续固定实际 Pack source、版本和整体摘要。

Plugin 是命令和 action 扩展方式。外部命令、HTTP adapter、Wasm action plugin 都不能绕过主 CLI；插件 manifest 中的权限会先被校验，返回的变更还要走 policy 和 patch。Wasm 是默认运行时；原生插件必须显式授权且只能在 Rainy 项目内运行，以保证执行记录写入项目审计日志。

### 7. 发布、安装和自更新

Release workflow 在 tag 或手动触发时运行：

```text
verify release inputs
build Linux/macOS/Windows binaries
package archives and sha256 files
force embedded-resource smoke, verify all expected assets, and generate SBOM/provenance
publish GitHub Release assets, checksums, and installer scripts
```

安装脚本根据当前系统选择对应 asset：

- Linux x86_64
- Linux arm64
- macOS Intel
- macOS Apple Silicon
- Windows x64

CLI 自更新通过 `rainy self check/update/skip` 管理。默认仓库来自 Cargo package repository，也可以通过 `--repo` 或 `RAINY_UPDATE_REPO` 覆盖。版本检查使用原生 HTTPS、标准 SemVer、超时和失败退避；指定版本更新固定使用对应 tag 的安装器，并在执行前通过该 Release 的 `installers.sha256` 验证安装脚本。

## 数据与协议

- `rainy.yaml`: 项目配置入口，包含路径、registry source、policy、verify。
- `capability.lock`: 已安装能力的事实来源，记录 provider、版本、Pack source/digest、artifacts、skills。
- `ExecutionPlan`: 可保存和重放的能力变更计划。
- `ChangeSet`: 文件级变更集合，包含 before/after、kind、summary、noop。
- `schemas/*.schema.json`: 对外协议和报告结构的稳定参考。
- `.rainy/audit.log`: 命令成功或失败的本地审计记录。
- JSON 输出：所有主要命令支持 `--json`，供 Agent、MCP、CI 调用。
- Release assets: 多平台 CLI 包、对应 sha256 文件和安装脚本。

## 当前限制与后续建设

### P0 / 近期

- 架构文档此前缺位。README 有使用说明，最终形态设计稿偏愿景，但缺少当前实现版架构说明；本文档补齐这一层。
- Release 与安装链路要求 tag/Cargo/binary 版本一致，并强制 checksum。正式 Release 仍需在 GitHub 上完成五平台资产验收。
- README 中测试数量和建设进度容易过期；当前已改为描述性说明，并提供 `make production-check` 作为生产可用性本地门禁入口。后续可增加 CI badge 或脚本生成状态。

### P1 / 中期

- 当前 Rust 实现仍是单 crate 多模块，和最终设计稿的多 crate 分层有差距。建议在接口稳定后拆分 `core/config/registry/plan/actions/policy/plugin/json-protocol`。
- `verify` 已区分 `local` 和 `ci`：local 适合本地开发，ci 是严格质量门禁。后续可以继续扩展 profile schema，例如显式声明 strict、timeout、required tools。
- MCP 和 Backstage 已补充部署、权限、版本兼容和打包说明，但实现仍是示例级，尚未发布为独立可安装包。
- Pack 完整性 manifest 可配合 cosign 发布者签名；组织仍需维护受信公钥轮换和撤销流程。

### P2 / 长期

- 发布渠道仍集中在 GitHub Release。可补 crates.io、Homebrew、npm 或包管理器 tap，并统一安装文档。
- audit log 具备预检、文件锁和落盘同步，但仍是本地文件；企业场景需要集中化审计、指标、trace id 贯通和 SIEM/日志平台集成。
- Capability registry 已具备协议限制、超时、大小上限、逐文件摘要和原子缓存替换；后续仍可增加离线镜像治理、公钥轮换和撤销分发。
- 企业审批系统、密钥系统、权限平台、starter 生态都需要通过私有 pack/plugin 落地，本仓库只提供协议和示例。
