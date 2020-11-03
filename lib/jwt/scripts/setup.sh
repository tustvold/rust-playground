#!/usr/bin/env bash

set -e

if [ ! -f "resources/secret.pem" ]; then
  mkdir -p resources
  openssl genpkey -out resources/secret.pem -algorithm RSA -pkeyopt rsa_keygen_bits:2048
  openssl rsa -in resources/secret.pem -pubout -out resources/public.pem
fi
