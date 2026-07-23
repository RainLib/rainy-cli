# 企业能力接入 Rainy CLI

企业内容不应硬编码进 Rainy CLI。Rainy 核心提供确定性的 plan、policy、apply、verify、
evidence 和 audit 边界；企业仓库负责定义能力、分发内容和连接内部平台。

## 内容应该放在哪里

| 企业内容 | Rainy 承载方式 | 典型内容 |
| --- | --- | --- |
| 可声明的项目变更 | Capability Pack | 依赖、配置、模板、CI、Helm、可观测、SDK |
| 能力发现与版本分发 | 私有 Registry | pack 版本、文件 URL、SHA-256 |
| 强制安全边界 | 分层 Policy | 禁止路径、审批 action、可编辑范围 |
| 外部系统调用 | Wasm Plugin / HTTPS Adapter | CMDB、IAM、审批、制品、部署平台 |
| 研发方法与模型约束 | 企业 Skill + Rainy Skill | 操作顺序、术语、升级流程、值班规则 |
| 交付事实 | Lock / Evidence / Audit | 版本、digest、变更、验证、trace ID |

不要把真实 secret 放入 pack、`rainy.yaml`、Skill 或生成模板。Pack 只生成 secret 引用；
凭据由 CI identity、Vault/KMS 或企业 secret 平台在运行时注入。

## 推荐仓库结构

```text
enterprise-rainy/
  packs/
    platform-baseline/
      pack.yaml
      capabilities/
      templates/
  plugins/
    approval-adapter/
      plugin.json
  registry/
    index.json
  policies/
    org-policy.yaml
  skills/
    company-engineering/
      SKILL.md
  tests/
```

Rainy 主仓库的 [examples/enterprise](../examples/enterprise) 提供一个可本地执行的最小样例。

## 1. 定义 Capability Pack

一个 capability 表达一个可独立安装、验证、升级和审计的业务能力。边界应以使用者能理解的
结果命名，例如 `company-observability`、`company-oidc`，不要按内部脚本文件拆分。

```bash
rainy schema validate --schema capability-pack \
  --file examples/enterprise/packs/company-platform-baseline/pack.yaml
rainy schema validate --schema capability \
  --file examples/enterprise/packs/company-platform-baseline/capabilities/company-platform-baseline.yaml
rainy conformance check --path examples/enterprise/packs
```

Capability 使用 Rainy 内置 action 处理结构化文件和模板；只有内置 action 无法表达的操作才交给
plugin。Pack 自身的 `policy` 只能收紧该 capability 的写入和审批边界，不能绕过系统 deny。

## 2. 配置分层 Policy

Rainy 按以下顺序读取 policy，并合并 deny、allow 和审批 action：

1. `/etc/rainy/policy.yaml`：机器镜像或受管开发环境。
2. `~/.rainy/policy.yaml`：用户环境。
3. `<workspace>/.rainy/org-policy.yaml`：仓库内企业门禁。
4. `<workspace>/rainy.yaml`：项目声明。
5. capability 自身 policy：单项能力约束。

`denyEdit` 和 `requireApproval` 在各层累积，内置敏感路径与危险命令禁止规则无法被下层覆盖。
当前 `allowEdit` 也是累积集合，因此企业级“绝对禁止”必须写入 `denyEdit`，不能只依靠一个较窄的
`allowEdit`。原生插件信任只允许在项目 `rainy.yaml` 或显式 CLI/env 授权中设置，组织 policy
不会静默开启宿主进程执行。

```bash
rainy schema validate --schema org-policy \
  --file examples/enterprise/project/.rainy/org-policy.yaml
```

生产环境应由模板或策略控制器生成 `/etc/rainy/policy.yaml`，并通过仓库保护阻止普通开发者修改
`.rainy/org-policy.yaml`。

## 3. 发布并关联私有 Registry

企业不需要 fork 或重新编译 Rainy CLI。把 Pack、Skill 和 Plugin 作为企业内容仓库发布，然后在每个
项目中关联一个或多个命名 Registry。支持 GitHub/GitLab Git 仓库、HTTPS 压缩包、HTTPS index
和本地目录；同一个项目可按名称选择 Registry，并按 Pack 名称只同步需要的模块。

```yaml
capabilityRegistry:
  sources:
    - type: git
      name: platform
      priority: 100
      url: https://gitlab.example.com/platform/rainy-packs.git
      ref: v1.4.0
    - type: archive
      name: security
      url: https://packages.example.com/rainy/security-v2.1.0.tar.gz
      sha256: <64_HEX_SHA256>
```

推荐通过命令维护配置，不手工编辑 YAML：

```bash
rainy registry add platform \
  git+https://gitlab.example.com/platform/rainy-packs.git --ref v1.4.0 --apply
rainy registry add security \
  https://packages.example.com/rainy/security-v2.1.0.tar.gz --sha256 <SHA256> --apply

rainy registry sync platform --module service-baseline,observability --dry-run
rainy registry sync platform --module service-baseline,observability --apply
rainy registry sync --all-registries --all --apply
rainy pack update --apply
```

远程内容不会写入当前工程。默认缓存位于
`~/.rainy/registries/<REGISTRY>/<SOURCE_HASH>`；可用 `RAINY_HOME` 修改根目录。项目只提交
`rainy.yaml` 与 `.rainy/registry.lock`，后者固定 resolved Git commit、内容 digest、模块列表、
缓存路径和已安装 Skill。`registry remove` 只解除当前项目关联，不删除其他项目可能复用的共享缓存。

压缩包必须是 `.tar.gz`、`.tgz` 或 `.zip`。未传 `--sha256` 时 Rainy 读取 `<URL>.sha256`；校验失败、
路径穿越、符号链接、特殊文件或解压大小超限都会拒绝安装，并保留上一份可用缓存。HTTP index 则逐文件
校验 SHA-256 和 Pack 身份。企业网关还应提供 TLS、身份认证、不可变版本目录和审计日志。

## 4. 接入审批、IAM 和部署平台

选择顺序如下：

1. 能返回 Rainy ChangeSet 的纯逻辑扩展：Wasm action plugin。
2. 已有企业服务：HTTPS adapter，服务端完成 IAM、审批和业务校验。
3. 只有无法沙箱化的本机工具才使用原生 plugin，并要求单独人工信任。

Plugin 请求的 read/write 权限必须最小化。返回的文件变更仍由 Rainy 执行 policy 和事务式 apply；
plugin 不应自行写项目目录。部署、数据库迁移和 secret 写入应使用
`policy.requireApproval` 中的稳定 action ID，并在外部审批系统中保存同一个 `--trace-id`。

## 5. 将企业规范提供给模型

企业 Skill 放在 Pack 的 `exports.skills` 中，Rainy 只安装显式声明的 Skill，并记录文件摘要。安装时
必须选择生效平台；Codex/Universal 使用 `.agents/skills`，Claude 使用 `.claude/skills`，Cursor
使用 `.cursor/skills`：

```bash
rainy registry sync platform --module company-engineering \
  --install-skills --target codex,cursor --apply
```

已安装 Skill 被人工修改后，更新会以 `REGISTRY_SKILL_CONFLICT` 停止；审查后才可使用 `--force` 覆盖。
企业 Skill 负责解释公司术语、服务目录和操作顺序，但不得绕过 Rainy。建议让 Agent 执行：

```bash
rainy --workspace <PROJECT_DIR> agent context --json
rainy --workspace <PROJECT_DIR> capability list --json
rainy --workspace <PROJECT_DIR> doctor --json
```

变更仍使用“保存 plan → 人工/系统审批 → apply 相同 plan → ci verify → evidence”。企业 Skill
可以随私有 Pack 发布，其来源、目标目录和 digest 由 `.rainy/registry.lock` 记录。
Rainy 内置模型规则见 [integrations/skills/rainy-cli](../integrations/skills/rainy-cli)。

## 6. CI/CD 门禁

私有 pack 仓库至少执行：

```bash
rainy schema validate --schema capability-pack --file packs/<PACK>/pack.yaml
rainy schema validate --schema capability --file packs/<PACK>/capabilities/<CAPABILITY>.yaml
rainy conformance check --path packs --json
rainy pack verify packs/<PACK>
```

消费项目至少执行：

```bash
rainy doctor --json
rainy verify --profile ci --json
rainy evidence generate --format all --json
```

把 `rainy.yaml`、`capability.lock`、`rainy-skills.yaml`、`skills.lock` 和 pack source 纳入代码审查；
把 `.rainy/audit.log`、evidence 和 trace ID 发送到集中审计系统。

## 当前边界与待建设项

- Policy 已支持机器、用户、仓库、项目和 capability 分层，但没有远程策略拉取与签名；企业应通过
  受管镜像/仓库分发，并使用 `denyEdit` 表达不可覆盖边界。
- HTTP registry 支持 checksum 和原子缓存切换，但目前没有内置 OIDC/mTLS 凭据协议；认证应由
  企业反向代理、短期 URL 或网络边界承担。
- Pack 有完整性 manifest 和可选 cosign 发布者签名，但没有内置透明日志或撤销列表。
- Audit 当前是项目本地文件，集中 SIEM、指标和跨服务 trace 需要外部采集。
- MCP 和 Backstage 目录是可运行示例，不是已发布的企业产品包；正式使用前需完成身份、配额、
  租户隔离和部署运维。
- 企业 starter、审批、IAM、secret 和部署适配器属于私有实现，不应进入 Rainy 开源核心。

这些边界不影响本地 pack、policy、plan/apply、verify 和 evidence 主流程，但在对外声明“生产可用”
时必须作为部署前置条件落实。
