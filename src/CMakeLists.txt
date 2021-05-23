cmake_minimum_required(VERSION 3.13)

set(PROJECT_NAME bose-connect-app-linux)

# Set the project name and version.
project(${PROJECT_NAME} VERSION 1.0)

# Specify the C standard.
set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED True)


# ------------------------------------------------------
# Check Pkg Config.
# ------------------------------------------------------
find_package(PkgConfig REQUIRED)

if (NOT PKG_CONFIG_FOUND)
    message(FATAL_ERROR "The 'pkgconf' package is not found.")
endif ()


# ------------------------------------------------------
# Check BlueZ
# ------------------------------------------------------
pkg_search_module(BLUEZ REQUIRED bluez)

if (NOT BLUEZ_FOUND)
    message(FATAL_ERROR "Please install `libbluetooth-dev`.")
elseif (NOT BLUEZ_VERSION)
    message(FATAL_ERROR "The version of Bluez is not correct: ${BLUEZ_VERSION}.")
endif ()

pkg_check_modules(BLUEZ REQUIRED bluez)
include_directories(${BLUEZ_INCLUDE_DIR})
link_directories(${BLUEZ_LIBRARIES})


# ------------------------------------------------------
# Code quality and security tools.
# ------------------------------------------------------
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
# TODO: Uncomment these lines and fix all the problems in local.
#set(CMAKE_C_CLANG_TIDY clang-tidy;)
#set(CMAKE_C_CLANG_TIDY clang-tidy; -fix; -fix-errors;)
#set(CMAKE_C_CPPCHECK
#        cppcheck;
#        --language=c;
#        --std=c11;
#        --platform=unix64;
#        --library=boost.cfg;
#        --library=cppcheck-lib.cfg;
#        --library=cppunit.cfg;
#        --library=gnu.cfg;
#        --library=libcerror.cfg;
#        --library=posix.cfg;
#        --library=std.cfg;
#        --enable=all;
#        --inconclusive;
#        --force;
#        --max-ctu-depth=1000000;
#        --project=${PROJECT_BINARY_DIR}/compile_commands.json;)

# ------------------------------------------------------
# Add the executable.
# ------------------------------------------------------
add_compile_definitions(PROGRAM_NAME="${PROJECT_NAME}")
add_executable(
        ${PROJECT_NAME}
        main.c
        main.h
        library/based.c
        library/based.h
        library/bluetooth.c
        library/bluetooth.h
        library/util.c
        library/util.h)
target_link_libraries(${PROJECT_NAME} bluetooth)
