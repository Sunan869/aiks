#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:?usage: ./scripts/push-service-image.sh <version>}"
IMAGE="${AIKS_IMAGE_NAME:?AIKS_IMAGE_NAME is required}"

FULL_IMAGE="${IMAGE}:${VERSION}"

echo "Pushing ${FULL_IMAGE}"
docker push "${FULL_IMAGE}"

echo "Pushed ${FULL_IMAGE}"
