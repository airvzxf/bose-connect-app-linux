#!/usr/bin/env sh
set -e

TOKEN="ABC_DEF"

echo "# ----------------------------------------------------"
echo "# Installation on RHEL -> Amazon Linux 2"
echo "# ----------------------------------------------------"

echo ""
echo "# Create directory and set up runner"
echo "# ----------------------------------------------------"
mkdir -p ~/github/actions-runner
cd ~/github/actions-runner || (
  echo "Error: Going to the actions runner folder."
  exit
)
curl \
  -o actions-runner-linux-x64-2.278.0.tar.gz \
  -L https://github.com/actions/runner/releases/download/v2.278.0/actions-runner-linux-x64-2.278.0.tar.gz
tar xzf ./actions-runner-linux-x64-2.278.0.tar.gz
./config.sh \
  --url https://github.com/airvzxf/bose-connect-app-linux \
  --token "${TOKEN}"
