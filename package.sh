#!/usr/bin/env bash

set -e

podman build -f Containerfile.distroless -t ghcr.io/ffimnsr/midas-rs:latest .
