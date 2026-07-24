# 企业 Git 能力仓库制作规范

本文说明企业如何在 GitHub、GitLab 或其他标准 Git 服务中制作、验证、发布和消费 Rainy 内容。
目标是让企业能力独立于 Rainy CLI 发版，同时保持版本可追踪、变更可预览、写入受 Policy 控制、
结果可验证和审计。

## 1. 先选择正确的仓库类型

Rainy 支持两类用途不同的 Git 仓库。

| 仓库类型 | 用途 | 消费入口 | 是否推荐日常业务使用 |
| --- | --- | --- | --- |
| 企业 Capability Registry | 业务能力、团队 Skill、可选 Plugin | `rainy registry add/sync` | 是 |
| 企业 Defaults 镜像 | 替换官方基础 Packs、Rainy Skills、Golden Path 模板 | `rainy defaults install/update` | 仅平台团队维护 |

普通业务团队应该制作 Capability Registry。只有需要内网镜像、统一修改官方基础能力或完全隔离公网时，
才制作 Defaults 镜像。企业不需要 fork Rainy CLI，也不需要重新编译自己的 `rainy` 命令。

## 2. 推荐的企业 Registry 仓库结构

Registry 根目录可以直接是一个 Pack，也可以包含多个 Pack。多模块仓库推荐如下：

```text
company-rainy-registry/
  README.md
  service-baseline/
    pack.yaml
    capabilities/
      service-baseline.yaml
    templates/
      service-metadata/
        service-metadata.yaml.hbs
    skills/
      company-service/
        SKILL.md
        references/
          platform-rules.md
    plugins/
      approval-adapter/
        plugin.json
        approval.wasm
  observability/
    pack.yaml
    capabilities/
      company-observability.yaml
  policies/
    org-policy.yaml
  tests/
    fixtures/
  scripts/
    verify.sh
  .github/workflows/validate.yml
  .gitlab-ci.yml
```

Rainy 要求 Registry source 指向“Pack 根目录”或“直接包含多个 Pack 目录的目录”，因此多模块仓库
不要在 Pack 外再增加 `packs/` 包装层。`--module` 使用
`pack.yaml` 中的 `metadata.name`，不是目录名；两者应保持一致。

模块拆分原则：

- 生命周期、Owner 和兼容性相同的能力放在同一个 Pack。
- 需要独立授权、独立发布或独立回滚的内容拆成不同 Pack。
- 一个 Registry 配置只有一个 Git `ref`，同次同步的所有模块共享该 commit。需要真正独立版本线时，
  应拆成不同仓库或不同 Registry source，不要只依靠 monorepo 中的多个 tag 命名。
- Policy 不应伪装成 Capability；Plugin 不应承载可由声明式 Action 完成的文件修改。

建议仓库同时维护 `CODEOWNERS`、贡献说明、变更日志和安全响应说明。平台团队负责 Pack 协议和发布，
领域团队负责能力语义及验证，安全团队负责 Policy 和 Plugin 权限，消费项目 Owner 负责审阅 plan 与升级。

## 3. Pack 清单

每个模块必须包含 `pack.yaml`：

```yaml
apiVersion: rainy.dev/v1
kind: CapabilityPack

metadata:
  name: service-baseline
  version: 1.2.0
  owner: platform-engineering
  description: Company service dependencies, configuration, CI, and model guidance.

requires:
  rainy: ">=0.4.0, <0.5.0"

exports:
  capabilities:
    - capabilities/service-baseline.yaml
  validators: []
  skills:
    - skills/company-service
  plugins:
    - plugins/approval-adapter
```

约束：

- `metadata.name` 在同一个 Registry 中必须唯一。
- `metadata.version` 使用 SemVer，并与 Git tag 对应。
- `requires.rainy` 应声明经过验证的 CLI 范围，不要无限放宽。
- `exports.*` 必须使用 Pack 内相对路径，禁止绝对路径和 `..`。
- `capabilities` 指向 Capability YAML。
- `skills` 指向包含 `SKILL.md` 的目录。
- `plugins` 指向包含 `plugin.json` 的目录；目前用于 conformance，Plugin 仍需单独执行
  `rainy plugin install`。
- `validators` 当前为协议保留字段。实际项目验证写在 Capability 的 `validations` 和 `doctor` 中。

先验证清单：

```bash
rainy schema validate --schema capability-pack \
  --file service-baseline/pack.yaml
rainy conformance check --path . --json
```

发布前应在独立的 fixture 项目中用本地 Registry 验证，避免把 `rainy.yaml` 写入能力仓库本身：

```bash
cd tests/fixtures/sample-service
rainy registry add company-dev /absolute/path/company-rainy-registry --apply
rainy registry sync company-dev --all --apply
rainy add capability service-baseline --dry-run
```

## 4. Capability 定义

Capability 应表达用户能理解的完整结果，例如 `company-observability`、`company-oidc`、
`service-baseline`，不要按单个脚本或单个文件拆分。

```yaml
apiVersion: rainy.dev/v1
kind: Capability

id: service-baseline
name: Company Service Baseline
version: 1.2.0
status: stable
owner: platform-engineering
description: Adds approved dependencies, configuration, CI, and service metadata.

dependsOn: []

providers:
  - id: standard
    default: true
    requiredConfig: []
  - id: regulated
    default: false
    requiredConfig:
      - COMPANY_COMPLIANCE_PROFILE

inputs:
  serviceTier:
    type: string
    default: tier-2
  enableTracing:
    type: boolean
    default: true

actions:
  install:
    - id: add-company-bom
      uses: maven.addBom
      with:
        modulePath: apps/backend
        groupId: com.example.platform
        artifactId: company-dependencies
        version: 3.4.0

    - id: merge-service-config
      uses: yaml.merge
      with:
        file: apps/backend/src/main/resources/application.yml
        patch:
          company:
            service-tier: "{{ inputs.serviceTier }}"
            tracing-enabled: "{{ inputs.enableTracing }}"

    - id: render-service-metadata
      uses: template.render
      with:
        template: templates/service-metadata
        target: generated/company

validations:
  - id: backend-tests
    command: ./mvnw test
    workingDirectory: apps/backend

doctor:
  checks:
    - id: service-metadata-exists
      uses: file.exists
      with:
        path: generated/company/service-metadata.yaml

agentRules:
  - Use this capability instead of manually adding company platform dependencies.
  - Never place service credentials in generated files.

policy:
  allowEdit:
    - capability.lock
    - apps/backend/pom.xml
    - apps/backend/src/main/resources/application.yml
    - generated/company/**
  denyEdit:
    - "**/secrets/**"
    - "**/*.pem"
    - "**/*.key"
  requireApproval:
    - deploy.production
```

设计规则：

1. `id` 在一个项目加载的所有 Registry 中全局唯一；重复会返回 `CAPABILITY_DUPLICATE`。
2. `version` 与所属 Pack 版本同步，升级时必须说明兼容性和迁移影响。
3. `dependsOn` 只声明真实依赖，Rainy 会阻止缺失依赖和删除被依赖能力。
4. 多 Provider 必须有且只有一个默认项，或者要求调用方显式传 `--provider`。
5. 输入必须有明确类型和安全默认值；Secret 只接受引用，不提供明文默认值。
6. Action 只描述确定性项目变更；部署、审批、数据库迁移等高风险操作必须进入审批边界。
7. `doctor` 检查安装后的静态事实，`validations` 检查真实工具链和行为。
8. `policy` 只能收紧权限，不能绕过系统、用户或组织层 deny。

完整 Action 列表和模板变量见
[Capability Pack Authoring](capability-pack-authoring.md)。

## 5. 模板制作

模板放在 Pack 内，并通过 `template.render` 引用。推荐扩展名 `.hbs`，生成目标使用真实扩展名。

```yaml
apiVersion: company.example/v1
kind: ServiceMetadata
metadata:
  name: "{{ package.java }}"
spec:
  tier: "{{ inputs.serviceTier }}"
  generatedRoot: "{{ paths.generated }}"
```

当前可用变量包括：

- `paths.backend`
- `paths.frontend`
- `paths.generated`
- `paths.evidence`
- `package.java`
- `package.npmScope`
- `packagePath`
- `inputs.<NAME>`

模板使用严格模式。引用不存在的变量会在 plan 阶段失败，不会生成半成品文件。模板中不得包含真实
Token、证书、私钥或环境密码；只生成 Vault、KMS、Kubernetes Secret、CI Secret 等引用。

## 6. 企业 Skill 制作

Skill 用于向 Codex、Claude、Cursor 等模型说明企业术语、操作顺序和安全约束。它不应直接绕过 Rainy
修改项目。

```text
skills/company-service/
  SKILL.md
  references/
    service-catalog.md
    incident-policy.md
  scripts/
    read-only-check.sh
```

`SKILL.md` 最小示例：

```markdown
---
name: company-service
description: Manage company service baselines through reviewed Rainy plans.
---

# Company Service

1. Run `rainy defaults status --json` and `rainy doctor --json`.
2. Use `rainy add capability service-baseline --dry-run --json` first.
3. Never infer approval for `--apply`.
4. After approval, apply the saved plan and run CI verification and evidence generation.
5. Stop on policy, checksum, lock, or local-drift errors.
```

安装企业 Skill：

```bash
rainy registry sync company \
  --module service-baseline \
  --install-skills \
  --target codex,claude,cursor \
  --apply
```

目标目录：

| Target | 项目目录 |
| --- | --- |
| `universal` / `codex` | `.agents/skills` |
| `claude` | `.claude/skills` |
| `cursor` | `.cursor/skills` |
| `github-copilot` | `.github/skills` |
| `gemini` | `.gemini/skills` |
| `opencode` | `.opencode/skills` |

Rainy 在 `.rainy/registry.lock` 中记录 Skill digest。本地内容被修改后，更新会停止并返回
`REGISTRY_SKILL_CONFLICT`；只有审查修改后才使用 `--force`。

## 7. Plugin 和企业服务

只有内置 Action 无法表达时才使用 Plugin。优先级：

1. 内置声明式 Action。
2. Wasm Plugin。
3. HTTPS Adapter。
4. 明确信任后的原生宿主进程 Plugin。

最小 Wasm Plugin 清单：

```json
{
  "protocolVersion": "rainy.plugin.v1",
  "name": "approval-adapter",
  "version": "1.0.0",
  "description": "Creates reviewed approval metadata changes.",
  "actions": [
    {
      "id": "approval.request",
      "description": "Request an enterprise approval",
      "runtime": "wasm",
      "wasm": "approval.wasm"
    }
  ],
  "permissions": {
    "fs": {
      "read": ["rainy.yaml", "generated/company/**"],
      "write": ["generated/company/approval.json"]
    },
    "network": [],
    "secrets": []
  }
}
```

验证和安装：

```bash
rainy schema validate --schema plugin-manifest \
  --file service-baseline/plugins/approval-adapter/plugin.json
rainy conformance check --path service-baseline/plugins/approval-adapter --json
rainy plugin install service-baseline/plugins/approval-adapter --dry-run
rainy plugin install service-baseline/plugins/approval-adapter --apply
```

当前 Registry 同步会下载 Plugin export 并执行 conformance，但不会自动安装 Plugin。平台必须在审查权限后
显式执行 `rainy plugin install`。HTTPS Adapter 的认证由企业网关、工作负载身份或短期凭据承担；不要把
长期凭据放进 `plugin.json`。

## 8. Policy 分发

Policy 和 Capability 的职责不同：Capability 描述能力变更，Policy 描述不可绕过的组织边界。

```yaml
allowEdit:
  - capability.lock
  - generated/company/**
denyEdit:
  - "**/.env*"
  - "**/secrets/**"
  - "**/*.pem"
  - "**/*.key"
requireApproval:
  - db.migrate
  - deploy.production
  - secret.write
```

加载层级：

1. `/etc/rainy/policy.yaml`
2. `~/.rainy/policy.yaml`
3. `<PROJECT>/.rainy/org-policy.yaml`
4. `<PROJECT>/rainy.yaml`
5. Capability `policy`

Registry 不会把仓库中的 `policies/` 自动写入项目。组织 Policy 应通过受管开发镜像、配置管理、项目
Golden Path 或独立策略控制器分发，并使用仓库保护阻止普通开发者修改。

```bash
rainy schema validate --schema org-policy --file policies/org-policy.yaml
```

## 9. 企业 Defaults 镜像

企业 Defaults 镜像用于替换 Rainy 官方基础内容，不用于承载所有业务能力。根目录必须包含：

```text
company-rainy-defaults/
  rainy-defaults.yaml
  community-packs/
  integrations/skills/
    rainy-cli/SKILL.md
    rainy-comet/SKILL.md
  defaults/templates/
```

清单示例：

```yaml
apiVersion: rainy.dev/v1
kind: RainyDefaults
metadata:
  name: company-rainy-defaults
  version: 0.4.0-company.1
requires:
  rainy: ">=0.4.0, <0.5.0"
paths:
  packs: community-packs
  skills: integrations/skills
  templates: defaults/templates
```

验证：

```bash
rainy schema validate --schema rainy-defaults --file rainy-defaults.yaml
rainy conformance check --path community-packs --json
```

消费：

```bash
export RAINY_DEFAULTS_SOURCE=https://gitlab.example.com/platform/rainy-defaults.git
export RAINY_DEFAULTS_REF=v0.4.0-company.1

rainy defaults install --dry-run
rainy defaults install --apply
rainy defaults doctor
```

Defaults 缓存位于 `~/.rainy/defaults/rainy-official/<SOURCE_HASH>`，锁位于
`~/.rainy/defaults.lock`。`RAINY_OFFLINE=1` 禁止网络回源。企业镜像必须保留 `rainy-cli` 和
`rainy-comet` 两个 Rainy 管理的 Skill，否则 `rainy skill init` 会失败。

## 10. Git 发布和版本策略

推荐规则：

- `main` 只接收 Pull/Merge Request。
- 每个 Pack 使用 SemVer。
- 发布 tag 不可变，例如 `registry-v1.2.0` 或仓库统一 `v1.2.0`。
- 消费项目固定 tag 或 commit，不直接跟踪可变 `main`。
- Rainy 会把 Git ref 解析为 commit 并记录到 `.rainy/registry.lock`。
- Breaking change 提升 major；新增兼容能力提升 minor；修复提升 patch。
- 每次发布记录新增能力、行为变化、迁移步骤、废弃项和最低 Rainy 版本。

同一仓库中的 Pack 统一发布时，所有变更 Pack 一起提升版本并共用仓库 tag。若 Pack 需要不同的发布
节奏，将其拆到独立 Registry；不要让一个可变分支同时代表多个无法复现的生产版本。

为每个 Pack 生成完整性清单；生产发布建议同时使用 cosign 企业密钥签名：

```bash
rainy pack sign service-baseline
rainy pack verify service-baseline

RAINY_PACK_SIGNING_KEY="$COSIGN_KEY_REF" rainy pack sign service-baseline
RAINY_PACK_TRUSTED_PUBLIC_KEY="$COSIGN_PUBLIC_KEY" rainy pack verify service-baseline
```

`.rainy-pack-signature.json` 和可选的 `.rainy-pack-signature.sig` 属于发布内容，应在 tag 前提交。私钥本身
只能由 CI Secret/KMS 提供，不得进入仓库。

创建 tag：

```bash
git tag -a v1.2.0 -m "Company Rainy Registry v1.2.0"
git push origin main
git push origin v1.2.0
```

生产 Git 服务应启用：

- Protected branch 和 protected tag。
- 必须通过 CI 才能合并。
- 至少一名平台 Owner 审查。
- Commit/tag 签名或企业签名策略。
- Secret scanning、依赖扫描、SAST 和制品保留策略。
- 禁止强制覆盖已发布 tag。

## 11. GitHub Actions 门禁示例

```yaml
name: validate-rainy-registry

on:
  pull_request:
  push:
    tags: ["v*"]

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - name: Install Rainy
        run: curl -fsSL https://github.com/RainLib/rainy-cli/releases/download/v0.4.0/install.sh | RAINY_VERSION=v0.4.0 sh
      - name: Validate packs
        run: |
          for pack in */pack.yaml; do
            ~/.rainy/bin/rainy schema validate --schema capability-pack --file "$pack"
          done
          ~/.rainy/bin/rainy conformance check --path . --json
      - name: Validate organization policy
        if: ${{ hashFiles('policies/org-policy.yaml') != '' }}
        run: ~/.rainy/bin/rainy schema validate --schema org-policy --file policies/org-policy.yaml
```

真实生产 CI 还应创建临时项目，并至少覆盖：

- 每个 Capability 的 dry-run、apply、doctor 和严格 verify。
- 对同一版本重复 apply，确认没有非幂等变更。
- 从上一支持版本升级到当前版本，并验证 lock 和生成文件。
- Provider、必填配置、缺失依赖、重复 ID 和模板冲突等失败路径。
- Skill 本地漂移、Plugin 权限越界、Policy deny 和 require-approval 边界。

## 12. GitLab CI 门禁示例

```yaml
stages: [validate]

validate-rainy-registry:
  stage: validate
  image: ubuntu:24.04
  before_script:
    - apt-get update && apt-get install -y curl git ca-certificates
    - curl -fsSL https://github.com/RainLib/rainy-cli/releases/download/v0.4.0/install.sh | RAINY_VERSION=v0.4.0 sh
  script:
    - find . -name pack.yaml -print0 | xargs -0 -n1 ~/.rainy/bin/rainy schema validate --schema capability-pack --file
    - ~/.rainy/bin/rainy conformance check --path . --json
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
    - if: $CI_COMMIT_TAG
```

## 13. Archive 和 HTTP Registry 发布

无法直接开放 Git 时，可以发布压缩包：

```bash
tar -czf company-rainy-registry-v1.2.0.tar.gz service-baseline observability
sha256sum company-rainy-registry-v1.2.0.tar.gz \
  > company-rainy-registry-v1.2.0.tar.gz.sha256
```

消费：

```bash
rainy registry add company-release \
  https://packages.example.com/rainy/company-rainy-registry-v1.2.0.tar.gz \
  --sha256 <SHA256> --apply
rainy registry sync company-release --all --apply
```

也可以生成 `rainy.registry.v1` HTTP index。每个文件必须列出相对路径和 SHA-256，Pack 名称与版本
必须和下载后的 `pack.yaml` 一致。格式见
[`schemas/registry-index.schema.json`](../schemas/registry-index.schema.json)。

## 14. 项目消费流程

关联多个 Registry：

```bash
rainy registry add platform \
  git+https://gitlab.example.com/platform/rainy-platform-registry.git \
  --ref v1.2.0 --priority 100 --apply

rainy registry add security \
  git+https://gitlab.example.com/security/rainy-security-registry.git \
  --ref v2.0.1 --priority 90 --apply
```

选择模块：

```bash
rainy registry sync platform \
  --module service-baseline,observability --dry-run
rainy registry sync platform \
  --module service-baseline,observability --apply
rainy registry sync platform --all --apply
rainy registry sync --all-registries --all --apply
```

安装能力：

```bash
rainy capability list --json
rainy add capability service-baseline --provider standard \
  --output-plan plans/service-baseline.json --dry-run
rainy apply --plan plans/service-baseline.json --dry-run
rainy apply --plan plans/service-baseline.json --apply
rainy doctor --capability service-baseline --json
rainy verify --profile ci --capability service-baseline --json
rainy evidence generate --format all --json
```

应提交到项目 Git：

- `rainy.yaml`
- `.rainy/registry.lock`
- `capability.lock`
- `rainy-skills.yaml`（启用项目 Skill profile 时）
- `skills.lock`（启用项目 Skill profile 时）
- 审阅后的 plan 和 evidence（按组织政策）

不应提交：

- `~/.rainy/defaults` 或 `~/.rainy/registries` 缓存
- `.rainy/audit.log`
- Secret、Token、私钥和本机凭据

## 15. 更新和回滚

更新前：

```bash
rainy registry list --verbose
rainy registry doctor platform --json
rainy registry add platform \
  git+https://gitlab.example.com/platform/rainy-platform-registry.git \
  --ref v1.3.0 --priority 100 --dry-run
```

更新配置中的 Git ref 后同步：

```bash
rainy registry add platform \
  git+https://gitlab.example.com/platform/rainy-platform-registry.git \
  --ref v1.3.0 --priority 100 --apply
rainy registry sync platform --module service-baseline,observability --apply
```

需要按 `.rainy/registry.lock` 中原有模块选择更新全部已关联 Registry 时，可改用
`rainy pack update --apply`。当前 `rainy pack update --dry-run` 不访问远端，只返回空变更预览；检查新版本
应先用 `registry add ... --ref <NEW_REF> --dry-run`，再由变更流程批准目标 ref。

回滚时重新关联旧 tag 并同步。Rainy 使用临时目录下载和校验，成功后才原子替换缓存；下载、校验或
解压失败时保留上一份有效缓存。Capability 已写入项目的变更仍应通过版本控制 PR 回滚，不要直接删除
`capability.lock`。

## 16. 发布前检查表

- [ ] Pack 和 Capability Schema 全部通过。
- [ ] `rainy conformance check` 通过。
- [ ] Pack 名、目录名、Capability ID 没有冲突。
- [ ] 所有导出路径均为安全相对路径。
- [ ] 每个 Action 都能生成稳定、幂等的 dry-run。
- [ ] Provider、依赖、输入默认值和错误场景均有测试。
- [ ] `doctor` 和 `validations` 覆盖安装结果。
- [ ] Capability Policy 只授予必要写路径。
- [ ] Skill 不会自行批准 `--apply` 或绕过 Policy。
- [ ] Plugin 权限最小化，原生 Plugin 有单独信任流程。
- [ ] 仓库、模板、Skill、Plugin 不包含 Secret。
- [ ] 发布 tag 不可变，并保留提交签名和 CI 证据。
- [ ] 消费项目已完成 plan、apply、doctor、strict verify 和 evidence 验证。

## 17. 当前实现边界

- 多 Registry 支持优先级排序，但重复 Capability ID 不会静默覆盖，必须由企业消除冲突。
- Registry 可自动安装导出的 Skill，但不会自动安装导出的 Plugin。
- `validators` 是保留字段；项目验证使用 Capability `validations` 和 `doctor`。
- Registry 中的 Policy 文件不会自动分发到项目或系统目录。
- 普通 Registry Pack 的 `requires.rainy` 当前用于声明和审查，CLI 尚未自动执行兼容范围拦截；CI 必须
  使用声明范围内的最低和最高支持版本验证。Defaults 镜像会执行该兼容检查。
- Git 认证复用系统 `git` 的 credential helper、SSH agent 和企业网络配置。
- Rainy 当前记录 resolved commit 和内容 digest；生产环境仍应启用 Git tag 签名、仓库保护和企业审计。
- 集中审计、IAM、审批、Secret 平台和部署平台需要通过企业基础设施或 Plugin/Adapter 接入。

最小可运行参考实现位于
[`examples/enterprise`](../examples/enterprise)，官方默认分发清单位于
[`rainy-defaults.yaml`](../rainy-defaults.yaml)。
