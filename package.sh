#!/usr/bin/env bash

set -e

if ! command -v docker; then
    podman build -f Containerfile.distroless -t ghcr.io/ffimnsr/midas-rs:latest .
else
    docker build -f Containerfile.distroless -t ghcr.io/ffimnsr/midas-rs:latest .
fi
