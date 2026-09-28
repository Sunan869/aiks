#!/usr/bin/env bash
set -euo pipefail

TAG="${1:-weknora-collector}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLATFORM="${AIKS_PLATFORM:-linux/amd64}"
IMAGE_REPO="${AIKS_COLLECTOR_IMAGE_REPO:-aiks-service}"
IMAGE="${IMAGE_REPO}:${TAG}"
OUT_DIR="${AIKS_IMAGE_OUT_DIR:-${ROOT_DIR}/dist/images}"
OUT_FILE="${OUT_DIR}/aiks-collector-${TAG}.tar"

BASE_IMAGES=(
  "rust:1.98.1-bookworm"
  "debian:bookworm-slim"
)

mkdir -p "$OUT_DIR"

echo "[INFO] target platform: $PLATFORM"
echo "[INFO] pulling official base images first"
for base in "${BASE_IMAGES[@]}"; do
  docker pull --platform "$PLATFORM" "$base"
done

echo "[INFO] building $IMAGE using local base images"
docker build \
  --platform "$PLATFORM" \
  --pull=false \
  -t "$IMAGE" \
  -f "${ROOT_DIR}/deploy/weknora/Dockerfile" \
  "$ROOT_DIR"

echo "[INFO] saving $OUT_FILE"
docker save -o "$OUT_FILE" "$IMAGE"

echo "[OK] $OUT_FILE"
echo "Upload it to: deploy/server/images/"
