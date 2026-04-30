#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
OUTPUT_DIR="${REPO_ROOT}/target/wheels"
IMAGE="${MATURIN_IMAGE:-ghcr.io/pyo3/maturin:latest}"
PLATFORM="${DOCKER_PLATFORM:-linux/arm64}"

show_help() {
    cat <<'EOF'
Build Linux ARM64 wheels for aras_py using Docker.

Usage:
  ./build-linux-arm64-wheel.sh [maturin build args]

Examples:
  ./build-linux-arm64-wheel.sh
  ./build-linux-arm64-wheel.sh -i python3.11
  ./build-linux-arm64-wheel.sh --compatibility manylinux_2_34

Notes:
  - The wheel is written to target/wheels at the repository root.
  - If no interpreter is specified, the script uses --find-interpreter.
  - Override the Docker image with MATURIN_IMAGE.
  - Override the Docker platform with DOCKER_PLATFORM.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    show_help
    exit 0
fi

has_interpreter_arg=false
for arg in "$@"; do
    case "$arg" in
        -i|--interpreter|--find-interpreter)
            has_interpreter_arg=true
            break
            ;;
    esac
done

mkdir -p "${OUTPUT_DIR}"

maturin_args=(build --release --out /io/target/wheels)
if [[ "${has_interpreter_arg}" == false ]]; then
    maturin_args+=(--find-interpreter)
fi
maturin_args+=("$@")

echo "Building aras_py wheel for ${PLATFORM} using ${IMAGE}"
echo "Output directory: ${OUTPUT_DIR}"

docker run --rm \
    --platform "${PLATFORM}" \
    -v "${REPO_ROOT}:/io" \
    -w /io/aras_py \
    "${IMAGE}" \
    "${maturin_args[@]}"
