#!/usr/bin/env bash
set -euo pipefail

TAG="${1:-weknora-collector}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLATFORM="${AIKS_PLATFORM:-linux/amd64}"
IMAGE_REPO="${AIKS_COLLECTOR_IMAGE_REPO:-aiks-service}"
IMAGE="${IMAGE_REPO}:${TAG}"
OUT_DIR="${AIKS_IMAGE_OUT_DIR:-${ROOT_DIR}/dist/images}"
OUT_FILE="${OUT_DIR}/aiks-collector-${TAG}.tar"

mkdir -p "$OUT_DIR"

echo "[INFO] building $IMAGE for $PLATFORM (base images via docker.1ms.run)"
docker buildx build \
  --platform "$PLATFORM" \
  --load \
  -t "$IMAGE" \
  -f "${ROOT_DIR}/deploy/weknora/Dockerfile" \
  "$ROOT_DIR"

echo "[INFO] saving $OUT_FILE"
docker save -o "$OUT_FILE" "$IMAGE"

echo "[OK] $OUT_FILE"
echo "Upload it to: deploy/server/images/"
