# 发布指南

本文档说明如何将 `tauri-plugin-pg-sync` 发布到 crates.io 和 npm。

## 前置准备

### 1. 账号注册

- **crates.io**: 访问 https://crates.io 使用 GitHub 登录
- **npm**: 访问 https://www.npmjs.com 注册账号

### 2. 配置认证

```bash
# crates.io 登录
cargo login --registry crates-io

# npm 登录
npm login --registry https://registry.npmjs.org
```

### 3. 更新元数据

编辑以下文件中的作者信息：

- `Cargo.toml`: `authors`, `repository`
- `guest-js/package.json`: `author`, `repository`, `name`（可能需要改为你自己的 npm scope）

## 发布流程

### 步骤 1: 验证 Rust 代码

```bash
cd tauri-plugin-pg-sync

# 检查代码格式
cargo fmt --check

# 运行 clippy 检查
cargo clippy -- -D warnings

# 运行测试
cargo test

# 验证打包
cargo package --list
```

### 步骤 2: 发布到 crates.io

```bash
# 干运行（不实际发布）
cargo publish --dry-run --registry crates-io

# 正式发布
cargo publish --registry crates-io
```

### 步骤 3: 构建并发布 JavaScript 绑定

```bash
cd guest-js

# 安装依赖
npm install

# 构建
npm run build

# 检查将要发布的文件
npm pack --dry-run

# 发布到 npm
npm publish --access public
npm publish --access public --registry https://registry.npmjs.org
```

## 版本管理

### 语义化版本

遵循 [Semantic Versioning](https://semver.org/):

- **MAJOR** (1.0.0): 不兼容的 API 变更
- **MINOR** (0.1.0): 向后兼容的新功能
- **PATCH** (0.0.1): 向后兼容的 bug 修复

### 更新版本

```bash
# 同时更新 Rust 和 JS 版本
# Cargo.toml: version = "0.2.0"
# guest-js/package.json: "version": "0.2.0"
```

## 发布检查清单

- [ ] 更新 CHANGELOG.md
- [ ] 更新版本号 (Cargo.toml + package.json)
- [ ] 运行所有测试
- [ ] 确保 README 是最新的
- [ ] 创建 git tag: `git tag v0.1.0`
- [ ] 推送 tag: `git push origin v0.1.0`
- [ ] 发布到 crates.io
- [ ] 发布到 npm

## GitHub Actions 自动发布（可选）

创建 `.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  publish-crates:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Publish to crates.io
        run: cargo publish
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}

  publish-npm:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
          registry-url: 'https://registry.npmjs.org'
      - name: Build and publish
        working-directory: guest-js
        run: |
          npm install
          npm run build
          npm publish --access public
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
```

配置 GitHub Secrets:
- `CARGO_REGISTRY_TOKEN`: 从 https://crates.io/settings/tokens 获取
- `NPM_TOKEN`: 从 https://www.npmjs.com/settings/tokens 获取

## 使用者安装方式

发布后，用户可以这样安装:

```bash
# Rust 依赖
cargo add tauri-plugin-pg-sync

# JavaScript 依赖
npm install @your-scope/tauri-plugin-pg-sync
```

## 常见问题

### Q: crates.io 发布失败 "name has already been taken"

A: 更换一个唯一的包名，例如 `tauri-plugin-pg-sync-yourname`

### Q: npm 发布失败 "403 Forbidden"

A: 确保包名在你的 npm scope 下，例如 `@yourusername/tauri-plugin-pg-sync`

### Q: 如何撤回已发布的版本？

A: 
- crates.io: `cargo yank --version 0.1.0`
- npm: `npm unpublish @scope/package@0.1.0`（24小时内可撤回）
