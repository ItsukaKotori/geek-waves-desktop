#!/usr/bin/env bash
# 从本机 gradle 缓存导出 org.itsuka 构件到 ci-repo/(maven 布局),供 GitHub CI 解析(nexus.local 不可达)。
# 关键:每个构件目录生成 maven-metadata.xml——Gradle 对 file 仓库的 -SNAPSHOT 解析必须有 metadata(实测缺它报 Could not find)。
# 更新 itsuka starter 后需重跑本脚本并提交。
set -euo pipefail
CACHE="$HOME/.gradle/caches/modules-2/files-2.1/org.itsuka"
OUT="$(cd "$(dirname "$0")/.." && pwd)/ci-repo"
VERSION="1.0.0-SNAPSHOT"
STAMP="$(date -u +%Y%m%d%H%M%S)"
ARTIFACTS="itsuka-spring-boot-starter itsuka-web-spring-boot-starter itsuka-mybatis-plus-spring-boot-starter itsuka-redis-spring-boot-starter"
rm -rf "$OUT"; mkdir -p "$OUT"
for art in $ARTIFACTS; do
  src="$CACHE/$art/$VERSION"
  [ -d "$src" ] || { echo "缓存缺 $art($src)"; exit 1; }
  dst="$OUT/org/itsuka/$art/$VERSION"; mkdir -p "$dst"
  # SNAPSHOT 缓存可含多个构建(hash 目录),必须取 mtime 最新的——与 Gradle 的解析口径一致;
  # 曾因 find|cp 覆盖顺序打进旧构建,CI 用旧 jar 在启动期 INSERT 上炸(无请求上下文取登录用户)。
  newest() { find "$src" -name "$1" -exec stat -f "%m %N" {} + | sort -rn | head -1 | cut -d' ' -f2-; }
  jar_src="$(newest "$art-$VERSION.jar")"
  pom_src="$(newest "$art-$VERSION.pom")"
  [ -n "$jar_src" ] && [ -n "$pom_src" ] || { echo "$art 导出不完整(缺 jar 或 pom)"; exit 1; }
  cp "$jar_src" "$dst/" && cp "$pom_src" "$dst/"
  cat > "$dst/maven-metadata.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>org.itsuka</groupId>
  <artifactId>$art</artifactId>
  <version>$VERSION</version>
  <versioning>
    <snapshot>
      <snapshotVersions>
        <snapshotVersion><extension>jar</extension><value>$VERSION</value></snapshotVersion>
        <snapshotVersion><extension>pom</extension><value>$VERSION</value></snapshotVersion>
      </snapshotVersions>
    </snapshot>
    <lastUpdated>$STAMP</lastUpdated>
  </versioning>
</metadata>
EOF
done
echo "ci-repo 就绪:"; find "$OUT" -type f | sort
