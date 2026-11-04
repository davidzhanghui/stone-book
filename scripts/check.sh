#!/bin/sh
# 提交前检查：格式、clippy 零警告、单元测试（含编辑内核随机测试与格式往返测试）。
# 本地与 CI 共用。更长的随机测试：STONEBROOK_FUZZ_SEEDS=2000 scripts/check.sh
set -e
cd "$(dirname "$0")/.."

printf '\n▸ cargo fmt --check\n'
cargo fmt --check

printf '\n▸ cargo clippy（-D warnings）\n'
cargo clippy --all-targets --locked -- -D warnings

printf '\n▸ cargo test\n'
cargo test --locked

printf '\n▸ vendored GPUI 的文字排版测试（含避头尾）\n'
(cd vendor/gpui && CARGO_TARGET_DIR="${GPUI_TEST_TARGET:-$PWD/../../target/gpui-tests}" \
  cargo test --no-default-features --features "font-kit runtime_shaders test-support" --lib text_system::line_wrapper)

printf '\n▸ 正式包配置（--no-default-features）能编译\n'
cargo check --no-default-features --locked

printf '\n✔ 全部检查通过\n'
