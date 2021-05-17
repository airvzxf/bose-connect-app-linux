#!/usr/bin/env sh
set -e

echo "# ----------------------------------------------------"
echo "# Set up on RHEL -> Amazon Linux 2"
echo "# ----------------------------------------------------"

echo ""
echo "# CMake3 - Install"
echo "# ----------------------------------------------------"
sudo yum --assumeyes install cmake3.x86_64
echo ""
echo "# CMake2 - Remove"
echo "# ----------------------------------------------------"
sudo yum --assumeyes remove cmake.x86_64
echo ""
echo "# CMake - List package"
echo "# ----------------------------------------------------"
sudo yum list cmake*

echo ""
echo "# BlueZ - Install"
echo "# ----------------------------------------------------"
sudo yum --assumeyes install bluez-libs-devel.x86_64
echo ""
echo "# BlueZ - List packages"
echo "# ----------------------------------------------------"
sudo yum list bluez*