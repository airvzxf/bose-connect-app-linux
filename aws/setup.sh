#!/usr/bin/env sh
set -e

echo "# ----------------------------------------------------"
echo "# Installation on RHEL -> Amazon Linux 2"
echo "# ----------------------------------------------------"

echo ""
echo "# Install CMake"
echo "# ----------------------------------------------------"
sudo yum --assumeyes install cmake3.x86_64
sudo yum --assumeyes remove cmake.x86_64
sudo yum list cmake*

echo ""
echo "# Install BlueZ"
echo "# ----------------------------------------------------"
sudo yum --assumeyes install bluez-libs.x86_64
sudo yum list bluez*
