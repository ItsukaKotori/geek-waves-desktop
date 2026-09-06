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
  find "$src" -name "$art-$VERSION.jar" -exec cp {} "$dst/" \;
  find "$src" -name "$art-$VERSION.pom" -exec cp {} "$dst/" \;
  [ -f "$dst/$art-$VERSION.jar" ] && [ -f "$dst/$art-$VERSION.pom" ] || { echo "$art 导出不完整"; exit 1; }
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
