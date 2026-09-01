cmake_minimum_required(VERSION 3.20)
project(VAmigaSpritePhaseCapture LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

set(VAMIGA_SOURCE "" CACHE PATH "Path to the root of an exact vAmiga source checkout")
if(NOT EXISTS "${VAMIGA_SOURCE}/Core/CMakeLists.txt")
    message(FATAL_ERROR "VAMIGA_SOURCE must name a vAmiga source root")
endif()

add_subdirectory("${VAMIGA_SOURCE}/Core" vamiga-core EXCLUDE_FROM_ALL)

add_executable(vamiga-sprite-phase-capture main.cpp)
target_link_libraries(vamiga-sprite-phase-capture PRIVATE VACore)

if(CMAKE_CXX_COMPILER_ID MATCHES "AppleClang|Clang|GNU")
    target_compile_options(
        vamiga-sprite-phase-capture
        PRIVATE
            -Wall
            -Wextra
            -Wpedantic
            -Werror
    )
endif()
