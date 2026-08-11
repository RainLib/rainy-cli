# Rainy Source 企业内容分发与版本管理

Rainy Source 是企业内容的统一分发入口。Git 仓库或 ZIP/TAR 包在根目录放置
`rainy-source.yaml` 后，Rainy 才会识别、校验、缓存和跟踪其中的模板、模块、Capability Pack、
Skill、Plugin 与 Defaults。企业不需要 fork 或重新编译 Rainy CLI。

Source 解决三个问题：

1. 接入前知道下载内容的身份、版本、兼容范围和类型。
2. 将远端 revision、发布版本、内容摘要和本地缓存关联起来。
3. 允许按内容 ID 选择模板或模块，不把整个企业仓库暴力复制到项目目录。

## 1. 分层边界

| 层 | 配置或清单 | 作用域 | 作用 |
| --- | --- | --- | --- |
| Source 分发层 | `rainy-source.yaml` | 企业内容仓库 | 声明一个分发包包含什么 |
| Source 用户配置 | `$RAINY_HOME/sources.yaml` | 当前用户 | 记录 Git、索引、Archive 或本地来源 |
| Source 用户锁 | `$RAINY_HOME/sources.lock` | 当前用户 | 固定 commit、版本、摘要和不可变缓存 |
| 项目来源锁 | `.rainy/project-source.lock` | 生成项目 | 记录项目使用的模板、模块和 Source 版本 |
| Capability Registry | `rainy.yaml`、`.rainy/registry.lock` | 项目 | 安装和更新 Capability Pack 与企业 Skill |
| 项目能力锁 | `capability.lock` | 项目 | 记录已应用能力和生成文件 |

Source 不替代 Registry。Source 管理“企业交付包从哪里来、包含什么、当前是哪一版”；Registry 管理
“这个项目启用了哪些 Capability Pack 和 Skill”。

## 2. 企业仓库结构

推荐使用一个 Source 仓库发布同一生命周期的一组内容：

```text
company-rainy-source/
├── rainy-source.yaml
├── templates/
│   ├── service-base/
│   │   ├── rainy.yaml.hbs
│   │   └── capability.lock.hbs
│   └── web-base/
├── modules/
│   ├── backend-java/
│   ├── frontend-nextjs/
│   └── delivery-gitlab/
├── packs/
│   ├── service-baseline/
│   │   └── pack.yaml
│   └── observability/
│       └── pack.yaml
├── skills/
│   └── company-delivery/
│       └── SKILL.md
├── plugins/
│   └── deployment-adapter/
│       └── plugin.json
└── defaults/
    └── company-defaults/
        └── rainy-defaults.yaml
```

最小清单：

```yaml
apiVersion: rainy.dev/v1
kind: RainySource
metadata:
  name: company-platform
  version: 1.4.0
  description: Company project templates and platform capabilities
requires:
  rainy: ">=0.5.0, <0.6.0"
contents:
  - id: service-base
    type: project-template
    path: templates/service-base
    required: true
  - id: backend-java
    type: workspace-module
    path: modules/backend-java
    defaultTarget: services/backend
  - id: observability
    type: capability-pack
    path: packs/observability
    version: 2.3.1
  - id: company-delivery
    type: skill
    path: skills/company-delivery
  - id: deployment-adapter
    type: plugin
    path: plugins/deployment-adapter
extensions:
  owner: platform-engineering
x-company-classification: internal
```

核心字段拒绝未知属性。企业扩展必须放入 `extensions`，或使用顶层 `x-*` 字段。路径必须是安全的
相对目录；符号链接、绝对路径和 `..` 路径穿越会被拒绝。

## 3. 内容类型与验证规则

| `type` | 必要内容 | Rainy 处理方式 |
| --- | --- | --- |
| `project-template` | 生成后必须存在有效的 `rainy.yaml` 和 `capability.lock` | `rainy new --source` 选择一个作为根模板 |
| `workspace-module` | `defaultTarget`；普通文件或 `.hbs` 模板 | `rainy new --source --module A,B` 组合多个模块 |
| `capability-pack` | `pack.yaml`，名称与内容 ID 相同 | 执行 Pack conformance，之后用 `source resolve` 交给 Registry |
| `skill` | `SKILL.md`，frontmatter `name` 与内容 ID 相同 | 校验 Skill；跨项目安装推荐通过 Pack `exports.skills` |
| `plugin` | `plugin.json` 或 `.rainy-plugin/plugin.json` | 校验协议、动作和最小权限，安装仍需显式审批 |
| `defaults` | `rainy-defaults.yaml`，`kind: RainyDefaults` | 校验和跟踪；替换官方默认包仍使用 `rainy defaults` |

一个 Source 可以声明多个 `project-template`，创建时只能选择一个。`workspace-module` 可以多选，
`required: true` 的模块会始终启用。多个内容渲染到同一路径时 Rainy 会失败，不会覆盖先前内容。

`.hbs` 文件的内容和路径支持以下变量：

- `{{ project.name }}`
- `{{ package.java }}`
- `{{ packagePath }}`

非 `.hbs` 文件按二进制原样复制。源仓库中的 `.git` 不进入缓存或生成项目。

## 4. Source 版本规则

`metadata.version` 必须是 SemVer，并代表整个 Source 的可消费快照。任何模板、模块、Pack、Skill、
Plugin 或 Defaults 行为变化，都必须提升 Source 版本。

- 兼容新增内容：提升 minor，例如 `1.4.0 -> 1.5.0`。
- 修复模板或规则：提升 patch，例如 `1.4.0 -> 1.4.1`。
- 删除内容、重命名 ID 或不兼容模板变量：提升 major。
- 带独立版本的内容同时更新 `contents[].version` 与自身清单版本。
- 同一个 SemVer 不得对应不同摘要；已发布 Tag、Archive 和索引条目必须不可变。

不同传输方式的版本感知能力不同：

| 来源 | 默认跟踪 | `source check` 如何判断更新 |
| --- | --- | --- |
| Git | `main`，建议生产固定 Tag | `git ls-remote` 比较远端 commit；新 SemVer 在下载校验后确认 |
| Rainy Source Index | `stable` channel 或指定 `--version` | 比较索引中的最高 SemVer 和 SHA-256 |
| 直接 Archive | 显式 `--sha256` 或 `<URL>.sha256` | 比较摘要；下载后读取清单版本 |
| 本地目录 | 当前目录内容 | 比较目录摘要并读取清单版本 |

Git 的 `main` 可以感知 commit 变化，但不能在不下载清单的情况下知道新 SemVer。生产发布优先使用
不可变 Tag 或 Rainy Source Index；开发环境可以继续跟踪 `main`。

## 5. 发布索引

OSS、对象存储或 CDN 应发布版本化 Archive 和一个小型索引：

```yaml
apiVersion: rainy.dev/v1
kind: RainySourceIndex
metadata:
  name: company-platform
releases:
  - version: 1.4.0
    url: releases/company-platform-1.4.0.zip
    sha256: <64_HEX_SHA256_FOR_STABLE_ARCHIVE>
    channel: stable
    notesUrl: https://git.example.com/platform/company-rainy-source/releases/v1.4.0
  - version: 1.5.0-rc.1
    url: releases/company-platform-1.5.0-rc.1.zip
    sha256: <64_HEX_SHA256_FOR_PREVIEW_ARCHIVE>
    channel: preview
```

索引 URL 和 Archive URL 必须使用 HTTPS；仅测试中的 loopback HTTP 例外。Archive 可将
`rainy-source.yaml` 放在压缩包根目录，或只包一层同名根目录。Rainy 会限制下载大小、解压大小、
条目数量，并拒绝符号链接和路径穿越。

## 6. 企业制作者操作步骤

在仓库根目录验证清单和所有声明内容：

```bash
rainy schema validate --schema rainy-source --file rainy-source.yaml
rainy source inspect .
```

Capability Pack 仍应单独执行 conformance：

```bash
rainy conformance check --path ./packs/service-baseline
rainy conformance check --path ./packs/observability
```

发布 Git Tag：

```bash
git tag -s v1.4.0 -m "Company Rainy Source 1.4.0"
git push origin v1.4.0
```

发布 Archive：

```bash
git archive --format=zip --prefix=company-platform/ \
  --output=company-platform-1.4.0.zip v1.4.0
shasum -a 256 company-platform-1.4.0.zip \
  > company-platform-1.4.0.zip.sha256
```

发布门禁至少包含：Schema、`source inspect`、每个 Pack conformance、Secret 扫描、Archive 摘要复算、
最低/最高支持 Rainy 版本测试，以及从空目录执行一次真实 `rainy new --source`。

## 7. 企业用户操作步骤

### 7.1 GitHub、GitLab 或企业 Git

先检查，不写配置：

```bash
rainy source inspect \
  git+ssh://git@git.example.com/platform/company-rainy-source.git \
  --ref v1.4.0
```

注册并写入用户级不可变缓存：

```bash
rainy source add company \
  git+ssh://git@git.example.com/platform/company-rainy-source.git \
  --ref v1.4.0 --apply
```

开发分支可以省略 `--ref`，此时默认跟踪 `main`。Git 认证使用 SSH agent 或系统 credential helper；
URL 中禁止 username/password、Token 和敏感查询参数。

### 7.2 ZIP/TAR 与发布索引

直接 Archive：

```bash
rainy source add company-release \
  https://packages.example.com/rainy/company-platform-1.4.0.zip \
  --sha256 <SHA256> --apply
```

稳定发布通道：

```bash
rainy source add company \
  https://packages.example.com/rainy/rainy-source-index.yaml \
  --channel stable --apply
```

固定索引中的一个版本：

```bash
rainy source add company-1-4 \
  https://packages.example.com/rainy/rainy-source-index.yaml \
  --version 1.4.0 --apply
```

### 7.3 查看、检查和更新

```bash
rainy source list
rainy source list --verbose
rainy source check company
rainy source check --all
rainy source update company          # 只检查，不写缓存
rainy source update company --apply  # 下载、验证并原子刷新
rainy source update --all --apply
```

远端暂时不可达且已有已验证缓存时，命令返回 warning，保留旧缓存，不阻塞后续 `rainy new --source`。
没有有效缓存时，创建或解析内容会失败，不会使用未验证数据。

### 7.4 从一个根模板组合多个企业模块

交互终端可以选择模板和模块：

```bash
rainy new order-service --source company
```

CI、Agent 和可复现脚本应显式选择：

```bash
rainy new order-service \
  --source company \
  --template service-base \
  --module backend-java,delivery-gitlab \
  --package com.company.orders \
  --git-url git@git.example.com:apps/order-service.git \
  --apply
```

生成过程使用临时目录，完成渲染并校验项目配置后才原子移动到目标路径。结束时只输出 Git 初始化、
remote、提交和推送命令，不会隐式创建或推送目标仓库。

### 7.5 使用 Pack、Plugin 和其他内容

解析已验证内容的绝对路径：

```bash
rainy source resolve company observability
rainy source resolve company observability --json
```

Capability Pack 交给项目 Registry：

```bash
PACK_PATH="$(rainy source resolve company observability --json \
  | jq -r '.data.report.sources[0].contents[0].resolvedPath')"
rainy registry add company-observability "$PACK_PATH" --apply
rainy registry sync company-observability --all --apply
rainy capability list
```

Plugin 必须单独审查权限后安装：

```bash
PLUGIN_PATH="$(rainy source resolve company deployment-adapter --json \
  | jq -r '.data.report.sources[0].contents[0].resolvedPath')"
rainy plugin install "$PLUGIN_PATH" --dry-run
rainy plugin install "$PLUGIN_PATH" --apply
```

企业 Skill 推荐放入 Capability Pack 的 `exports.skills`，再通过 Registry 的平台和 Skill 多选流程安装，
这样选择结果会进入项目锁。独立 `skill` 内容当前只执行身份、结构和摘要验证；Source 不会绕过
`rainy skill` 的宿主选择和漂移检查直接复制它。`defaults` 内容同理，替换官方默认包必须通过
`RAINY_DEFAULTS_SOURCE` 和 `rainy defaults install/update` 完成。

## 8. 项目版本感知与升级

Source 创建的项目会提交 `.rainy/project-source.lock`。在项目目录执行：

```bash
rainy source check --project
rainy source update --project
rainy source update --project --apply
rainy source check --project
```

项目锁保存不含凭据的原始地址、ref/channel 和摘要。其他开发者克隆项目后，即使用户级
`sources.yaml` 尚未注册该名称，也可以先执行 `source check --project`；显式执行
`source update --project --apply` 后会恢复该用户的验证缓存和 Source 关联。私有仓库认证仍来自本机
SSH agent 或 credential helper，不写入项目锁。Git、Index 和 Archive 地址可跨机器恢复；本地目录
记录的是规范化绝对路径，换机器后必须重新 `source add` 到该机器可访问的位置。

可能状态：

- `current`：项目锁、用户缓存和远端 revision 一致。
- `update-available`：远端 Source 已变化，先执行 `source update --project --apply`。
- `project-update-available`：用户缓存已经更新，但项目仍锁定旧模板或模块版本。
- `unreachable`：远端不可达；已有缓存和项目仍可使用。

`source update` 只更新用户级不可变缓存，不会覆盖已经生成并可能被业务修改的项目文件。当前版本没有
自动三方合并模板升级；项目应创建对照工程或读取新内容，评审差异后通过 PR 显式迁移。这个限制是为了
避免模板升级破坏业务代码。

## 9. 自动化与退出语义

所有结果支持 `--json`，外层协议为 `rainy.command.v1`，Source 报告为
`rainy.source-report.v1`。CI 推荐：

```bash
rainy source check --all --json > source-check.json
rainy source resolve company observability --json > resolved-content.json
```

- 当前、预览和带可用缓存的网络 warning 退出 `0`。
- 参数或清单错误退出 `2`。
- 无缓存、下载、认证或运行错误按错误类别退出非零。
- 批量同步中某个来源不可达时逐项报告；有旧缓存则保留，首次同步失败则整体报告 failed。

## 10. 回滚与清理

回滚 Git 来源时重新注册旧 Tag；回滚索引来源时固定旧 `--version`：

```bash
rainy source add company \
  git+ssh://git@git.example.com/platform/company-rainy-source.git \
  --ref v1.3.2 --apply

rainy source add company \
  https://packages.example.com/rainy/rainy-source-index.yaml \
  --version 1.3.2 --apply
```

移除关联：

```bash
rainy source remove company
rainy source remove company --apply
```

删除关联不会立即删除按摘要存储的共享缓存，避免破坏仍引用该快照的项目。缓存清理命令和自动模板迁移
尚未提供；需要纳入组织运维流程时，应先根据 `sources.lock` 和项目来源锁完成引用审计。

可运行的最小仓库见 [`examples/enterprise-source`](../examples/enterprise-source)，机器协议见
[`schemas/rainy-source.schema.json`](../schemas/rainy-source.schema.json) 和
[`schemas/rainy-source-index.schema.json`](../schemas/rainy-source-index.schema.json)。
