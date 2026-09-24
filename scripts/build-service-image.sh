#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:?usage: ./scripts/build-service-image.sh <version>}"
IMAGE="${AIKS_IMAGE_NAME:-aiks-service}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

FULL_IMAGE="${IMAGE}:${VERSION}"

echo "Building ${FULL_IMAGE}"
docker build \
  -t "${FULL_IMAGE}" \
  -f "${ROOT_DIR}/deploy/team/Dockerfile" \
  "${ROOT_DIR}"

echo "Built ${FULL_IMAGE}"
