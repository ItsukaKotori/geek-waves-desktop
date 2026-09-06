#!/usr/bin/env bash
# 从本机 gradle 缓存导出 org.itsuka 构件到 ci-repo/(maven 布局),供 GitHub CI 解析(nexus.local 不可达)。
# 更新 itsuka starter 后需重跑本脚本并提交。
set -euo pipefail
CACHE="$HOME/.gradle/caches/modules-2/files-2.1/org.itsuka"
OUT="$(cd "$(dirname "$0")/.." && pwd)/ci-repo"
VERSION="1.0.0-SNAPSHOT"
ARTIFACTS="itsuka-spring-boot-starter itsuka-web-spring-boot-starter itsuka-mybatis-plus-spring-boot-starter itsuka-redis-spring-boot-starter"
rm -rf "$OUT"; mkdir -p "$OUT"
for art in $ARTIFACTS; do
  src="$CACHE/$art/$VERSION"
  [ -d "$src" ] || { echo "缓存缺 $art($src)"; exit 1; }
  dst="$OUT/org/itsuka/$art/$VERSION"; mkdir -p "$dst"
  find "$src" -name "$art-$VERSION.jar" -exec cp {} "$dst/" \;
  find "$src" -name "$art-$VERSION.pom" -exec cp {} "$dst/" \;
  [ -f "$dst/$art-$VERSION.jar" ] && [ -f "$dst/$art-$VERSION.pom" ] || { echo "$art 导出不完整"; exit 1; }
done
echo "ci-repo 就绪:"; find "$OUT" -type f | sort
