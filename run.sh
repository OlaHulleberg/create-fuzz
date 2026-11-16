#!/usr/bin/env bash
# create-fuzz harness. Usage:
#   ./run.sh server            start the Java side (NeoForge dedicated server with Create) and wait until ready
#   ./run.sh stop              stop it
#   ./run.sh fuzz [args...]    the Rust driver (see `./run.sh fuzz help`)
# Expects ../Create (a Create checkout, published with `./gradlew publishToMavenLocal`) and
# ../create-horizon next to this repository.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-21-openjdk-amd64}"
export FUZZ_DIR="${FUZZ_DIR:-$here/work}"
gradlew="${CREATE_DIR:-$here/../Create}/gradlew"
run="$here/java/run"

case "${1:-}" in
server)
    mkdir -p "$run" "$FUZZ_DIR"
    # A local test server only: never public, no mobs, a flat world.
    echo "eula=true" > "$run/eula.txt"
    [ -f "$run/server.properties" ] || cat > "$run/server.properties" <<'P'
level-type=minecraft\:flat
generator-settings={"layers"\:[{"block"\:"minecraft\:bedrock","height"\:1}],"biome"\:"minecraft\:the_void"}
generate-structures=false
spawn-monsters=false
spawn-animals=false
spawn-npcs=false
online-mode=false
server-ip=127.0.0.1
server-port=25599
view-distance=4
simulation-distance=4
max-tick-time=-1
P
    rm -f "$FUZZ_DIR/ready"
    (cd "$here/java" && nohup "$gradlew" --no-daemon -q runServer > "$FUZZ_DIR/server.log" 2>&1 & echo $! > "$FUZZ_DIR/server.pid")
    for _ in $(seq 600); do
        [ -f "$FUZZ_DIR/ready" ] && { echo "server ready"; exit 0; }
        kill -0 "$(cat "$FUZZ_DIR/server.pid")" 2>/dev/null || { echo "server exited; see $FUZZ_DIR/server.log"; exit 1; }
        sleep 1
    done
    echo "server not ready after 10 min"; exit 1 ;;
stop)
    pkill -f 'createfuzz|runServer' || true ;;
fuzz)
    shift
    cargo +1.99.1 run -q --release --manifest-path "$here/rust/Cargo.toml" -- "$@" ;;
*)
    sed -n 2,8p "$0"; exit 1 ;;
esac
