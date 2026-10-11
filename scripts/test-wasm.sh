#!/usr/bin/env bash
# Checks gearbox on the web target. Needs rustup (Homebrew's Rust can't build for wasm32).
#
#   scripts/test-wasm.sh                 compile-check anarchy, magician-vgpu and gearbox for
#                                        wasm32, then run anarchy's and gearbox's tests/wasm.rs in Node
#   scripts/test-wasm.sh --web [EXAMPLE] also build EXAMPLE (default: shapes) with Trunk and
#                                        smoke-test it in headless Chromium, saving a
#                                        screenshot to target/web-EXAMPLE.png
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET=wasm32-unknown-unknown
# rustup's cargo has to win over Homebrew's for the wasm target to be found
export PATH="$HOME/.cargo/bin:$PATH"

web=false
example=shapes
if [[ "${1:-}" == "--web" ]]; then
    web=true
    example="${2:-shapes}"
fi

missing=false
need() { # need COMMAND HOW_TO_INSTALL
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "missing $1, install it with: $2" >&2
        missing=true
    fi
}

need rustup "see https://rustup.rs"
if command -v rustup >/dev/null 2>&1 && ! rustup target list --installed | grep -qx "$TARGET"; then
    echo "missing the $TARGET target, install it with: rustup target add $TARGET" >&2
    missing=true
fi

# the test runner must match the wasm-bindgen crate version exactly
wasm_bindgen_version=$(cargo tree --target "$TARGET" -e normal -i wasm-bindgen --depth 0 2>/dev/null | head -1 | sed -E 's/^wasm-bindgen v([^ ]+).*/\1/')
runner_install="cargo install wasm-bindgen-cli --version $wasm_bindgen_version"
need wasm-bindgen-test-runner "$runner_install"
if command -v wasm-bindgen-test-runner >/dev/null 2>&1; then
    runner_version=$(wasm-bindgen-test-runner --version | awk '{print $2}')
    if [[ "$runner_version" != "$wasm_bindgen_version" ]]; then
        echo "wasm-bindgen-test-runner is $runner_version but the crate is $wasm_bindgen_version, reinstall with: $runner_install --force" >&2
        missing=true
    fi
fi
need node "brew install node"
if $web; then
    need trunk "cargo install trunk --locked"
    if [[ ! -d tests/web/node_modules ]]; then
        echo "missing Playwright, install it with: (cd tests/web && npm install && npx playwright install chromium)" >&2
        missing=true
    fi
fi
$missing && exit 1

echo "== anarchy: compile all targets, run tests/wasm.rs in Node"
(cd ../anarchy && cargo build --target "$TARGET" --all-targets && cargo test --target "$TARGET" --test wasm)
echo "== compile: magician-vgpu"
(cd ../shader_magician && cargo build -p magician-vgpu --target "$TARGET")
echo "== compile: gearbox (lib and examples)"
cargo build --target "$TARGET" --lib --examples

echo "== test: tests/wasm.rs in Node"
cargo test --target "$TARGET" --test wasm

if $web; then
    dist="target/web-dist-$example"
    echo "== web: trunk build --example $example"
    trunk build --example "$example" --dist "$dist"
    echo "== web: smoke test in headless Chromium"
    node tests/web/smoke.mjs --dist "$dist" --screenshot "target/web-$example.png"
fi

echo "wasm checks passed"
