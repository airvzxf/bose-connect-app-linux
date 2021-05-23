#!/usr/bin/env bash

cd ../
clang-format -i ./*.c ./*.h library/*.c library/*.h
