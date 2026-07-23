use crate::config::{self, LockedCapability};
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use chrono::Utc;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug)]
pub struct InitOptions {
    pub base_dir: PathBuf,
    pub name: String,
    pub package: String,
    pub preset: Option<String>,
    pub golden_path: Option<String>,
    pub dry_run: bool,
}

pub fn init_app(options: InitOptions) -> RainyResult<CommandOutput> {
    let preset = options.preset.as_deref().unwrap_or("spring-nextjs");
    if preset != "spring-nextjs" {
        return Err(RainyError::config(
            "PRESET_UNSUPPORTED",
            format!("unsupported preset: {preset}"),
        ));
    }
    let golden_path = options
        .golden_path
        .as_deref()
        .unwrap_or("spring-nextjs-saas");
    if golden_path != "spring-nextjs-saas" {
        return Err(RainyError::config(
            "GOLDEN_PATH_UNSUPPORTED",
            format!("unsupported golden path: {golden_path}"),
        ));
    }

    let project_dir = options.base_dir.join(&options.name);
    if project_dir.exists() && project_dir.join("rainy.yaml").exists() {
        return Err(RainyError::config(
            "PROJECT_ALREADY_EXISTS",
            format!(
                "{} already looks like a Rainy project",
                project_dir.display()
            ),
        ));
    }
    let package_path = options.package.replace('.', "/");
    let registry_path = config::default_registry_path()?;
    let mut files = Vec::new();

    write(
        &project_dir,
        "rainy.yaml",
        rainy_yaml(&options.name, &options.package),
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        "capability.lock",
        lock_yaml(&options.name, &registry_path)?,
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        "AGENTS.md",
        agents_md(&options.name),
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        "apps/backend/pom.xml",
        backend_pom(&options.name),
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        format!("apps/backend/src/main/java/{package_path}/DemoApplication.java"),
        demo_application(&options.package),
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        "apps/backend/src/main/resources/application.yml",
        "spring:\n  application:\n    name: demo-saas\n\nserver:\n  port: 8080\n",
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        "apps/backend/src/test/java/.gitkeep",
        "",
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        "apps/frontend/package.json",
        frontend_package_json(&options.name),
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        "apps/frontend/pnpm-lock.yaml",
        include_str!("../assets/frontend-pnpm-lock.yaml"),
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        "apps/frontend/app/page.tsx",
        "export default function Page() {\n  return <main>Rainy demo</main>;\n}\n",
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        "apps/frontend/src/components/.gitkeep",
        "",
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        "compose.yaml",
        "services:\n  backend:\n    build: ./apps/backend\n    ports:\n      - \"8080:8080\"\n  frontend:\n    build: ./apps/frontend\n    ports:\n      - \"3000:3000\"\n",
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        ".github/workflows/ci.yml",
        "name: ci\non:\n  pull_request:\n  push:\n    branches: [main]\njobs:\n  rainy:\n    runs-on: ubuntu-latest\n    timeout-minutes: 30\n    steps:\n      - uses: actions/checkout@v5\n      - uses: actions/setup-java@v4\n        with:\n          distribution: temurin\n          java-version: \"21\"\n      - name: Install Maven\n        run: sudo apt-get update && sudo apt-get install -y maven\n      - uses: pnpm/action-setup@v4\n        with:\n          version: \"10\"\n      - uses: actions/setup-node@v4\n        with:\n          node-version: \"22\"\n          cache: pnpm\n          cache-dependency-path: apps/frontend/pnpm-lock.yaml\n      - name: Install frontend dependencies\n        working-directory: apps/frontend\n        run: pnpm install --frozen-lockfile\n      - name: Install Rainy CLI\n        env:\n          RAINY_VERSION: v0.1.2\n        run: curl -fsSL https://github.com/RainLib/rainy-cli/releases/download/v0.1.2/install.sh | sh\n      - name: Verify Rainy project\n        run: ~/.rainy/bin/rainy verify --profile ci --json\n",
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        "openapi/.gitkeep",
        "",
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        "charts/.gitkeep",
        "",
        &mut files,
        options.dry_run,
    )?;
    write(
        &project_dir,
        "evidence/.gitkeep",
        "",
        &mut files,
        options.dry_run,
    )?;

    Ok(CommandOutput::Init {
        status: if options.dry_run {
            "dry-run"
        } else {
            "created"
        },
        project: options.name,
        path: project_dir.to_string_lossy().to_string(),
        files,
    })
}

fn write(
    project_dir: &std::path::Path,
    rel_path: impl AsRef<str>,
    content: impl AsRef<str>,
    files: &mut Vec<String>,
    dry_run: bool,
) -> RainyResult<()> {
    let rel_path = rel_path.as_ref();
    let path = project_dir.join(rel_path);
    if !dry_run {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content.as_ref())?;
    }
    files.push(rel_path.to_string());
    Ok(())
}

fn rainy_yaml(name: &str, package: &str) -> String {
    format!(
        r#"apiVersion: rainy.dev/v1
kind: Project

project:
  name: {name}
  type: fullstack
  owner: demo-team

stack:
  backend: spring-boot
  frontend: nextjs
  gateway: none
  auth: none
  database: none
  cache: none

paths:
  backend: apps/backend
  frontend: apps/frontend
  generated: generated
  evidence: evidence

package:
  java: {package}
  npmScope: "@demo"

capabilityRegistry:
  sources: []

policy:
  allowNativePlugins: false
  allowEdit:
    - rainy.yaml
    - capability.lock
    - AGENTS.md
    - .agents/skills/**
    - .claude/skills/**
    - .cursor/skills/**
    - .github/skills/**
    - .gemini/skills/**
    - .opencode/skills/**
    - .rainy/plugins/**
    - .rainy/registry.lock
    - apps/backend/src/**
    - apps/backend/pom.xml
    - apps/backend/src/main/resources/application.yml
    - apps/frontend/**
    - generated/**
    - compose.yaml
    - .github/workflows/**
    - .devcontainer/**
    - charts/**
    - openapi/**
    - evidence/**
  denyEdit:
    - "**/application-prod.yml"
    - "**/.env.production"
    - "**/secrets/**"
    - "**/*.pem"
    - "**/*.key"
    - "**/*.p12"
  requireApproval:
    - gateway.publish
    - k8s.apply
    - db.migrate
    - secret.write

verify:
  profiles:
    local:
      - doctor
      - docker-compose-config
      - backend-test
      - frontend-build
    ci:
      - doctor
      - backend-test
      - frontend-build
      - openapi-validate
      - security-basic
"#
    )
}

fn lock_yaml(project_name: &str, registry_path: &std::path::Path) -> RainyResult<String> {
    let mut lock = config::empty_lock(project_name);
    let now = Utc::now();
    let capabilities = [
        (
            "spring-boot-web",
            vec![
                "apps/backend/pom.xml",
                "apps/backend/src/main/resources/application.yml",
            ],
        ),
        (
            "nextjs-admin",
            vec![
                "apps/frontend/package.json",
                "apps/frontend/pnpm-lock.yaml",
                "apps/frontend/app",
            ],
        ),
        ("docker-compose-local", vec!["compose.yaml"]),
        ("github-actions-ci", vec![".github/workflows/ci.yml"]),
    ];
    let mut map = BTreeMap::new();
    for (id, artifacts) in capabilities {
        map.insert(
            id.to_string(),
            LockedCapability {
                version: "0.1.0".to_string(),
                provider: None,
                pack: format!("{id}@0.1.0"),
                source: Some(format!("builtin:{id}")),
                digest: crate::registry::pack_digest(&registry_path.join(id)).ok(),
                installed_at: now,
                artifacts: artifacts.into_iter().map(str::to_string).collect(),
            },
        );
        lock.skills.push(format!("{id}@0.1.0"));
    }
    lock.capabilities = map;
    config::save_lock_content(&lock)
}

fn agents_md(project: &str) -> String {
    format!(
        r#"<!-- rainy:context:start -->
# AGENTS.md

## Project Rules
- Use Rainy CLI for capability changes.
- Run `rainy add capability <id> --dry-run` before applying changes.
- Do not edit generated capability artifacts without updating evidence.

## Installed Capabilities
- spring-boot-web
- nextjs-admin
- docker-compose-local
- github-actions-ci

## Commands
- `rainy capability list`
- `rainy doctor`
- `rainy verify --profile local`
- `rainy verify --profile ci`
- `rainy evidence generate`

## Capability Usage
Prefer Rainy packs before manually wiring common infrastructure in {project}.
<!-- rainy:context:end -->
"#
    )
}

fn backend_pom(name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.example</groupId>
    <artifactId>{name}-backend</artifactId>
    <version>0.1.0-SNAPSHOT</version>
    <properties>
        <java.version>21</java.version>
        <spring-boot.version>3.3.0</spring-boot.version>
    </properties>
    <dependencies>
        <dependency>
            <groupId>org.springframework.boot</groupId>
            <artifactId>spring-boot-starter-web</artifactId>
            <version>${{spring-boot.version}}</version>
        </dependency>
    </dependencies>
</project>
"#
    )
}

fn demo_application(package: &str) -> String {
    format!(
        r#"package {package};

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;

@SpringBootApplication
public class DemoApplication {{
    public static void main(String[] args) {{
        SpringApplication.run(DemoApplication.class, args);
    }}
}}
"#
    )
}

fn frontend_package_json(name: &str) -> String {
    format!(
        r#"{{
  "name": "{name}-frontend",
  "version": "0.1.0",
  "private": true,
  "scripts": {{
    "build": "next build",
    "dev": "next dev"
  }},
  "dependencies": {{
    "next": "14.2.0",
    "react": "18.3.1",
    "react-dom": "18.3.1"
  }},
  "devDependencies": {{
    "typescript": "5.5.4"
  }}
}}
"#
    )
}
