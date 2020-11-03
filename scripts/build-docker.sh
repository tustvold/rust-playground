#!/usr/bin/env bash

#
# Builds a docker image for a single release binary
#

set -e

repo_root=$(realpath "$(git rev-parse --show-toplevel)")
binary=$1
image=$2

cat << EOF > "${repo_root}/.dockerignore"
*
!target/release/$binary
!services/$binary/Rocket.toml
EOF

docker build -t "${image}" --build-arg "BINARY=${binary}" "${repo_root}"
