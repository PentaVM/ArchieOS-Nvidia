cmake_minimum_required(VERSION 3.24)

project(archie-stage LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# ---------------------------------------------------------------------------
# archie-stage — the compositor Archie works in.
#
# Everything that decides something lives in `display/visual` and is tested there on a
# machine without a display: who holds the input, whether a command may run against the
# surface as it is now, how the pointer travels, what the camera frames. This project is
# the part that cannot be tested without a Wayland session — sockets, a seat, a window —
# and so it stays thin.
#
# Linux only: Qt Wayland Compositor has no other platform. Qt Wayland Compositor is
# available under GPL-3.0 or a commercial Qt licence, so a binary built from this directory
# is distributed under GPL-3.0 unless a commercial licence applies. The Apache-2.0 sources
# themselves are compatible with that.
# ---------------------------------------------------------------------------

add_subdirectory(${CMAKE_CURRENT_SOURCE_DIR}/../visual archie-visual)

find_package(Qt6 6.7 REQUIRED COMPONENTS Core)

add_library(archie-stage-codec STATIC src/CommandCodec.cpp src/CommandCodec.h)
target_include_directories(archie-stage-codec PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)
target_link_libraries(archie-stage-codec PUBLIC archie::visual Qt6::Core)
target_compile_options(archie-stage-codec PRIVATE
    $<$<CXX_COMPILER_ID:GNU,Clang>:-Wall;-Wextra;-Wpedantic;-Werror>
)

option(ARCHIE_STAGE_BUILD_COMPOSITOR "Build the compositor itself (needs Qt Wayland Compositor)" ON)

if(ARCHIE_STAGE_BUILD_COMPOSITOR)
    find_package(Qt6 6.7 REQUIRED COMPONENTS Gui Quick WaylandCompositor Network)
    # Typing goes through the compositor's private keyboard API (see src/KeyTyper.h). Newer
    # Qt ships that as a package of its own; older Qt defined it with the public module.
    find_package(Qt6 QUIET COMPONENTS WaylandCompositorPrivate)
    find_package(PkgConfig REQUIRED)
    pkg_check_modules(XKBCOMMON REQUIRED IMPORTED_TARGET xkbcommon>=1.0)
    qt_standard_project_setup(REQUIRES 6.7)

    qt_add_executable(archie-stage
        src/main.cpp
        src/KeyTyper.cpp
        src/KeyTyper.h
        src/StageController.cpp
        src/StageController.h
    )
    qt_add_qml_module(archie-stage
        URI Archie.Stage
        VERSION 1.0
        QML_FILES
            qml/Main.qml
    )
    # The ArchieOS mark (logo package v3.5, design by Xavi), shared with the shell.
    qt_add_resources(archie-stage "archie-stage-brand"
        PREFIX "/archie/brand"
        BASE "${CMAKE_CURRENT_SOURCE_DIR}/../shell/brand"
        FILES "${CMAKE_CURRENT_SOURCE_DIR}/../shell/brand/archieos-logo-dark.png"
              "${CMAKE_CURRENT_SOURCE_DIR}/../shell/brand/archieos-app-512.png")
    # The shell's words in the session's language, shared with the stage's captions.
    qt_add_resources(archie-stage "archie-stage-words"
        PREFIX "/archie/shell/prototype"
        BASE "${CMAKE_CURRENT_SOURCE_DIR}/../shell/prototype"
        FILES "${CMAKE_CURRENT_SOURCE_DIR}/../shell/prototype/Lang.js")
    target_link_libraries(archie-stage PRIVATE
        archie-stage-codec
        Qt6::Gui
        Qt6::Quick
        Qt6::WaylandCompositor
        Qt6::WaylandCompositorPrivate
        Qt6::Network
        PkgConfig::XKBCOMMON
    )
    target_compile_definitions(archie-stage PRIVATE
        QT_NO_CAST_FROM_ASCII
        QT_NO_CAST_TO_ASCII
    )
    install(TARGETS archie-stage RUNTIME DESTINATION bin)
endif()

option(ARCHIE_STAGE_BUILD_TESTS "Build the stage codec tests" OFF)

if(ARCHIE_STAGE_BUILD_TESTS)
    enable_testing()
    add_executable(test_codec tests/test_codec.cpp)
    target_link_libraries(test_codec PRIVATE archie-stage-codec)
    target_include_directories(test_codec PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../visual/tests)
    target_compile_definitions(test_codec PRIVATE
        ARCHIE_FIXTURES="${CMAKE_CURRENT_SOURCE_DIR}/../../protocols/fixtures"
    )
    target_compile_options(test_codec PRIVATE
        $<$<CXX_COMPILER_ID:GNU,Clang>:-Wall;-Wextra;-Wpedantic;-Werror>
    )
    add_test(NAME test_codec COMMAND test_codec)
endif()
