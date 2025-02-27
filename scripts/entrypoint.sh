#!/bin/sh
set -e

# Ensure the config directory exists
mkdir -p config

# Copy the docker secret into the expected file
cp /run/secrets/preagonal-client-config config/client.toml

# Run the compiled binary
exec ./target/release/preagonal-client-rs
