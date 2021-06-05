#!/usr/bin/env bash
set -ev

docker image rm \
  -f \
  airvzxf/bose-connect-app-linux-os:latest

docker build \
  . \
  --force-rm \
  --file Dockerfile \
  --tag airvzxf/bose-connect-app-linux-os:latest

docker image push \
  airvzxf/bose-connect-app-linux-os:latest
