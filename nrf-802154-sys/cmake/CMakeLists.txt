# Standalone host for Nordic's component CMake files. nrfxlib normally adds
# these from an nRF Connect SDK application, which supplies the project and the
# platform interface target.
cmake_minimum_required(VERSION 3.20)

if(NOT DEFINED NRF_LLVM_AR)
  message(FATAL_ERROR "NRF_LLVM_AR must point at llvm-ar")
endif()
# Set the archive tool before CMake initializes the C language. Passing
# CMAKE_AR as an untyped command-line cache variable is overwritten on macOS.
set(CMAKE_AR "${NRF_LLVM_AR}" CACHE FILEPATH "Target-independent archiver" FORCE)

project(nrf_802154_rust LANGUAGES C)

# Avoid the host ranlib. LLVM ar's `s` operation writes the index and works for
# every target object format (notably ARM ELF objects on a macOS build host).
set(CMAKE_C_ARCHIVE_CREATE "<CMAKE_AR> qc <TARGET> <LINK_FLAGS> <OBJECTS>")
set(CMAKE_C_ARCHIVE_FINISH "<CMAKE_AR> s <TARGET>")

if(NOT DEFINED NRF_802154_SOURCE_DIR)
  message(FATAL_ERROR "NRF_802154_SOURCE_DIR must point at the vendored Nordic driver")
endif()

add_library(nrf-802154-platform INTERFACE)
add_library(nrf-802154-driver-interface INTERFACE)
add_library(nrf-802154-sl INTERFACE)

add_subdirectory("${NRF_802154_SOURCE_DIR}/common" common)
add_subdirectory("${NRF_802154_SOURCE_DIR}/driver" driver)
