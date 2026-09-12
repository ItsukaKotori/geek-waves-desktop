# GeekWaves Desktop

GeekWaves 桌面端:Tauri 2 壳内嵌 jlink JRE + Spring Boot 后端,双击即用。设计见
`../docs/superpowers/specs/2026-09-05-geekwaves-desktop-design.md`,实现计划见
`../docs/superpowers/plans/2026-09-05-geekwaves-desktop.md`。

## 结构

- `src-tauri/` Rust 壳(进程编排、密钥、端口、首启/导入、错误页)
- `splash/` 壳内置首启/加载/错误页(非 Vue)
- `scripts/build.sh` 一键打包(组装 → jlink → 冒烟 → dmg);`--bundle-only` 供 CI / dev 前置
- `deps.json` 钉 frontend/backend 仓库与 ref(CI 按此 checkout)
- `ci/init-repos.gradle` CI 依赖仓库初始化:把仓库替换为 mavenCentral + GitHub Packages,从
  `https://maven.pkg.github.com/ItsukaKotori/itsuka-spring` 解析 `org.itsuka:*:1.0.0-SNAPSHOT`;
  需要仓库 secret `PACKAGES_READ_TOKEN`(read:packages PAT)

## 本机构建(macOS)

前置:Rust stable、Node 22、JDK 21、`cargo install tauri-cli --locked`,
兄弟仓库 `geek-waves-frontend`/`geek-waves-backend` 与本仓库同级,
`~/.gradle/gradle.properties` 配 `gpr.user`/`gpr.token`(GitHub Packages read:packages PAT)。

```bash
./scripts/build.sh          # 出 src-tauri/target/release/bundle/dmg/*.dmg
```

## 本地开发(壳本身)

```bash
./scripts/build.sh --bundle-only   # 组装资源(改动前后端后重跑)
cargo tauri dev                    # 起壳,splash → 后端 → 导航
cargo test --manifest-path src-tauri/Cargo.toml
```

前后端日常开发不变(vite dev + bootRun),不经过壳。

## 数据与密钥

- 数据目录:macOS `~/Library/Application Support/GeekWaves/`(Windows `%APPDATA%\GeekWaves\`,Linux `~/.local/share/GeekWaves/`)
- `crypto.key` 首启生成(0600)或随导入写入;后端仅监听 127.0.0.1,端口区间 8977-8999

## 发版(CI)

仓库双远端:Gitee canonical,GitHub 跑 CI。Gitee 建仓后补配远端:
`git remote add origin git@gitee.com:cnZuikaku/geek-waves-desktop.git && git push origin main`。

前置:gh auth login(发布阶段需要)。发版流程:

1. 前后端推 Gitee + GitHub(`git push origin && git push github --tags`)
2. 更新 `deps.json` 的 ref 到目标提交/tag → 提交推送
3. 打 tag:`git tag v0.x.y && git push github v0.x.y`
4. Actions 四平台矩阵出包并挂到 GitHub Release(windows msi / linux deb+AppImage / mac 双架构 dmg)
