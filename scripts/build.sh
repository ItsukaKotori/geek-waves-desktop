#!/usr/bin/env bash
# 组装桌面打包资源(webapp + app.jar + jlink runtime),可选冒烟与 tauri build。
# 用法:
#   ./scripts/build.sh               # 组装 + 冒烟 + cargo tauri build(出 dmg)
#   ./scripts/build.sh --bundle-only # 仅组装(供 CI 或 cargo tauri dev 前置)
# 环境变量:FRONTEND_DIR / BACKEND_DIR 覆盖兄弟仓库路径;JAVA_HOME 覆盖 jlink 用 JDK(须 21);
#           GRADLE_OFFLINE=1(默认)离线构建后端;GRADLE_EXTRA 追加 gradlew 参数(预留给 CI 传 init 脚本;
#           仅限空格分隔的 flag 类参数,经词分割展开,含空格的路径会碎)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FRONTEND_DIR="${FRONTEND_DIR:-$ROOT/../geek-waves-frontend}"
BACKEND_DIR="${BACKEND_DIR:-$ROOT/../geek-waves-backend}"
RESOURCES="$ROOT/src-tauri/resources"
# 模块清单:jdeps 对 fat jar(嵌套 jar)不可靠,采用固定清单 + 冒烟门槛兜底。
# 覆盖:Spring Boot web/webflux/flyway(H2 需 java.sql/naming)、OSHI(management/jdk.management/unsupported)、
# TLS(ec/cryptoki)、locale(中文日志/格式)、zipfs(部分库的 zip 文件系统)、
# java.desktop(冒烟实修:Spring 配置绑定走 java.beans.PropertyEditorSupport)、
# java.instrument(冒烟实修:Tomcat ClassFileTransformer)。
# 其余为爆炸 fat jar 后 jdeps --list-deps 实测引用:java.compiler/java.prefs/java.scripting/
# java.datatransfer/java.transaction.xa/java.management.rmi、jdk.attach/jdk.jdi/jdk.jfr/jdk.net
# (Netty/Tomcat/热插拔代理/扩展 socket 选项)。
MODULES="java.base,java.compiler,java.datatransfer,java.desktop,java.instrument,java.logging,java.management,java.management.rmi,java.naming,java.net.http,java.prefs,java.scripting,java.security.jgss,java.sql,java.sql.rowset,java.transaction.xa,java.xml,java.xml.crypto,jdk.attach,jdk.crypto.cryptoki,jdk.crypto.ec,jdk.jdi,jdk.jfr,jdk.localedata,jdk.management,jdk.net,jdk.unsupported,jdk.zipfs"
SMOKE_PORT=18982
GRADLE_OFFLINE="${GRADLE_OFFLINE:-1}"

log() { echo "[build-desktop] $*"; }

# --- 1. 前端 ---
log "构建前端($FRONTEND_DIR)"
( cd "$FRONTEND_DIR" && npm ci && npm run build )
mkdir -p "$RESOURCES"
rm -rf "$RESOURCES/webapp"
cp -R "$FRONTEND_DIR/dist" "$RESOURCES/webapp"

# --- 2. 后端 ---
log "构建后端($BACKEND_DIR)"
# GRADLE_FLAGS 用字符串而非数组:macOS 自带 /bin/bash 3.2 下 set -u + 空数组展开
# "${ARR[@]}" 会报 unbound variable(4.4 才修复),字符串 + 词分割两端兼容。
if [[ "$GRADLE_OFFLINE" == "1" ]]; then GRADLE_FLAGS="--offline"; else GRADLE_FLAGS=""; fi
( cd "$BACKEND_DIR" && ./gradlew --quiet $GRADLE_FLAGS ${GRADLE_EXTRA:-} bootJar )
JAR="$(ls "$BACKEND_DIR"/build/libs/geekwaves-server-*.jar | grep -v '\.original$' | head -1)"
cp "$JAR" "$RESOURCES/app.jar"

# --- 3. jlink runtime ---
if [[ -z "${JAVA_HOME:-}" ]]; then
  JAVA_BIN_PATH="$(command -v java)"
  JAVA_HOME="$(cd "$(dirname "$JAVA_BIN_PATH")/.." && pwd)"
fi
log "jlink 裁剪 JRE($JAVA_HOME)"
rm -rf "$RESOURCES/runtime"
"$JAVA_HOME/bin/jlink" \
  --add-modules "$MODULES" \
  --strip-debug --no-header-files --no-man-pages --compress zip-6 \
  --output "$RESOURCES/runtime"
# jlink 会把 legal/conf 等文件置为只读(444);tauri-build 的 copy_resources 用 fs::copy 保留权限,
# 资源变更后的二次构建会因覆盖只读目标文件而 EACCES(Task 3 本机实修)。统一补回属主写权限。
chmod -R u+w "$RESOURCES/runtime"

if [[ "${1:-}" == "--bundle-only" ]]; then
  log "仅组装完成:$RESOURCES"
  exit 0
fi

# --- 4. 冒烟:jlink runtime 启动 app.jar,验证 /api/ping 与 SPA 回退 ---
log "冒烟测试(端口 $SMOKE_PORT)"
SMOKE_DIR="$(mktemp -d)"
"$RESOURCES/runtime/bin/java" -jar "$RESOURCES/app.jar" \
  --server.port=$SMOKE_PORT --server.address=127.0.0.1 \
  --spring.datasource.url="jdbc:h2:file:$SMOKE_DIR/smoke;MODE=MySQL;DATABASE_TO_LOWER=TRUE;CASE_INSENSITIVE_IDENTIFIERS=TRUE" \
  --spring.web.resources.static-locations="file:$RESOURCES/webapp/" \
  --geekwaves.web.spa-fallback=true \
  > "$SMOKE_DIR/smoke.log" 2>&1 &
SMOKE_PID=$!
trap 'kill "$SMOKE_PID" 2>/dev/null || true; rm -rf "$SMOKE_DIR"' EXIT

ok=""
for _ in $(seq 1 120); do
  if curl -sf -o /dev/null "http://127.0.0.1:$SMOKE_PORT/api/ping"; then ok=1; break; fi
  if ! kill -0 "$SMOKE_PID" 2>/dev/null; then
    echo "冒烟失败:后端进程提前退出。日志:"; cat "$SMOKE_DIR/smoke.log"; exit 1
  fi
  sleep 0.5
done
[[ -n "$ok" ]] || { echo "冒烟失败:60s 未通过健康检查。日志尾部:"; tail -50 "$SMOKE_DIR/smoke.log"; exit 1; }

curl -sf "http://127.0.0.1:$SMOKE_PORT/tools" | grep -q "GeekWaves" \
  || { echo "冒烟失败:/tools 未回退到 index.html(SPA fallback 未生效)"; exit 1; }
log "冒烟通过(/api/ping + SPA 回退)"

# --- 5. tauri build(出 dmg)---
log "cargo tauri build"
cd "$ROOT"
cargo tauri build --bundles app,dmg
log "完成:src-tauri/target/release/bundle/dmg/"
