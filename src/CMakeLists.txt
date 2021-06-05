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
# Add the executable.
# ------------------------------------------------------
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
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
get_target_property(PROJECT_SOURCES ${PROJECT_NAME} SOURCES)


# ------------------------------------------------------
# CLang Format: Autoformatting
# ------------------------------------------------------
if (AUTOFORMATTING)
    add_custom_command(
            COMMENT "\n\nAutoformatting: CLang Format\n"
            TARGET ${PROJECT_NAME}
            PRE_BUILD
            COMMAND
            cd ${CMAKE_SOURCE_DIR} &&
            clang-format
            "--sort-includes"
            "--verbose"
            "--Werror"
            "-i"
            ${PROJECT_SOURCES}
            VERBATIM
    )
endif ()


# ------------------------------------------------------
# CLang Format
# ------------------------------------------------------
if (VALIDATE_QA)
    add_custom_command(
            COMMENT "\n\nScanning: CLang Format\n"
            TARGET ${PROJECT_NAME}
            PRE_BUILD
            COMMAND
            cd ${CMAKE_SOURCE_DIR} &&
            clang-format
            "--Werror"
            "--dry-run"
            ${PROJECT_SOURCES}
            VERBATIM
    )
endif ()

# ------------------------------------------------------
# CLang Tidy
# ------------------------------------------------------
if (VALIDATE_QA)
    add_custom_command(
            COMMENT "\n\nScanning: CLang Tidy\n"
            TARGET ${PROJECT_NAME}
            PRE_BUILD
            COMMAND
            cd ${CMAKE_SOURCE_DIR} &&
            clang-tidy
            -p ${CMAKE_BINARY_DIR}
            ${PROJECT_SOURCES}
            VERBATIM
    )
endif ()


# ------------------------------------------------------
# CPP Check
# ------------------------------------------------------
set(CPPCHECK_TEMPLATE "")
string(APPEND CPPCHECK_TEMPLATE "{file}:{line}:{column}\\n")
string(APPEND CPPCHECK_TEMPLATE "  Message: {message}\\n")
string(APPEND CPPCHECK_TEMPLATE "    Check: CWE-{cwe}\\n")
string(APPEND CPPCHECK_TEMPLATE "     Type: {severity} -> {id}\\n")
string(APPEND CPPCHECK_TEMPLATE "     Line: {line}:{column}\\n")
string(APPEND CPPCHECK_TEMPLATE "    Stack: {callstack}\\n")
string(APPEND CPPCHECK_TEMPLATE "{code}\\n")

set(CPPCHECK_TEMPLATE_LOCATION "")
string(APPEND CPPCHECK_TEMPLATE_LOCATION "{file}:{line}:{column}\\n")
string(APPEND CPPCHECK_TEMPLATE_LOCATION "   Note: {info}\\n")
string(APPEND CPPCHECK_TEMPLATE_LOCATION "   Line: {line}:{column}\\n")
string(APPEND CPPCHECK_TEMPLATE_LOCATION "{code}\\n")

if (VALIDATE_QA)
    add_custom_command(
            COMMENT "\n\nScanning: CPP Check\n"
            TARGET ${PROJECT_NAME}
            PRE_BUILD
            COMMAND cppcheck
            "--language=c"
            "--std=c11"
            "--platform=unix64"
            "--suppress=missingIncludeSystem"
            "--enable=all"
            "--inconclusive"
            "--force"
            "--max-ctu-depth=1000000"
            "--library=boost.cfg"
            "--library=cppcheck-lib.cfg"
            "--library=cppunit.cfg"
            "--library=gnu.cfg"
            "--library=libcerror.cfg"
            "--library=posix.cfg"
            "--library=std.cfg"
            "--template=${CPPCHECK_TEMPLATE}"
            "--template-location=${CPPCHECK_TEMPLATE_LOCATION}"
            "--project=${CMAKE_BINARY_DIR}/compile_commands.json"
            "--error-exitcode=1"
            VERBATIM
    )
endif ()
