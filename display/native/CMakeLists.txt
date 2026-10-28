cmake_minimum_required(VERSION 3.24)

project(archie-display LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# The semantic visual state model: what to show, decided without Qt. Kept in its own
# subproject so that the decisions worth testing can be tested on a machine with no GPU.
add_subdirectory(${CMAKE_CURRENT_SOURCE_DIR}/../visual archie-visual)
# Reading a real .osm.pbf extract into building footprints. Depends on zlib, which is
# exactly why it is its own library rather than living in archie-visual.
add_subdirectory(${CMAKE_CURRENT_SOURCE_DIR}/../osm archie-osm)

find_package(Qt6 6.6 REQUIRED COMPONENTS Quick QuickControls2 Quick3D Network Concurrent)
qt_standard_project_setup(REQUIRES 6.6)

# The delivered visual decisions are Rust; Qt remains the renderer/World host.
get_filename_component(ARCHIE_REPOSITORY_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../.." ABSOLUTE)
set(ARCHIE_LUMEN_DESIGN_DIR "${ARCHIE_REPOSITORY_ROOT}/core/crates/archie-lumen-design")
set(ARCHIE_LUMEN_CARGO_TARGET "${CMAKE_CURRENT_BINARY_DIR}/cargo")
set(ARCHIE_LUMEN_LIBRARY "${ARCHIE_LUMEN_CARGO_TARGET}/release/${CMAKE_STATIC_LIBRARY_PREFIX}archie_lumen_design${CMAKE_STATIC_LIBRARY_SUFFIX}")
find_program(ARCHIE_CARGO_EXECUTABLE cargo REQUIRED)
file(GLOB_RECURSE ARCHIE_LUMEN_SOURCES CONFIGURE_DEPENDS
    "${ARCHIE_LUMEN_DESIGN_DIR}/src/*.rs" "${ARCHIE_LUMEN_DESIGN_DIR}/src/*.json"
    "${ARCHIE_LUMEN_DESIGN_DIR}/include/*.h")
add_custom_command(OUTPUT "${ARCHIE_LUMEN_LIBRARY}"
    COMMAND ${CMAKE_COMMAND} -E env CARGO_BUILD_JOBS=2
        "${ARCHIE_CARGO_EXECUTABLE}" build --locked --release -p archie-lumen-design
        --manifest-path "${ARCHIE_REPOSITORY_ROOT}/core/Cargo.toml"
        --target-dir "${ARCHIE_LUMEN_CARGO_TARGET}"
    DEPENDS ${ARCHIE_LUMEN_SOURCES} "${ARCHIE_LUMEN_DESIGN_DIR}/Cargo.toml"
        "${ARCHIE_REPOSITORY_ROOT}/core/Cargo.toml" "${ARCHIE_REPOSITORY_ROOT}/core/Cargo.lock"
    COMMENT "Building the Rust Lumenfield design library" VERBATIM)
add_custom_target(archie-lumen-design-build DEPENDS "${ARCHIE_LUMEN_LIBRARY}")
add_library(archie-lumen-design STATIC IMPORTED GLOBAL)
set_target_properties(archie-lumen-design PROPERTIES IMPORTED_LOCATION "${ARCHIE_LUMEN_LIBRARY}")
add_dependencies(archie-lumen-design archie-lumen-design-build)
if(UNIX)
    find_package(Threads REQUIRED)
    target_link_libraries(archie-lumen-design INTERFACE ${CMAKE_DL_LIBS} Threads::Threads m)
elseif(WIN32)
    target_link_libraries(archie-lumen-design INTERFACE ws2_32 userenv bcrypt ntdll)
endif()

qt_add_executable(archie-display
    src/main.cpp
    src/ArchieBridge.cpp
    src/ArchieBridge.h
    src/ExplorationPreview.cpp
    src/ExplorationPreview.h
    src/ExplorationCredential.cpp
    src/ExplorationCredential.h
    src/GlobeStreamBridge.cpp
    src/GlobeStreamBridge.h
    src/EarthMeshes.cpp
    src/EarthMeshes.h
    src/GlobeTiles.cpp
    src/GlobeTiles.h
    src/WorldVisuals.cpp
    src/WorldVisuals.h
    src/WorldImageProvider.h
    src/MeshGeometry.cpp
    src/MeshGeometry.h
    src/Payload.cpp
    src/Payload.h
    src/LumenField.cpp
    src/LumenField.h
    src/XaviDesignBridge.cpp
    src/XaviDesignBridge.h
    src/LumenGeometry.cpp
    src/LumenGeometry.h
    src/AppCatalog.cpp
    src/AppCatalog.h
    src/DisplayControl.cpp
    src/DisplayControl.h
    src/ShellRuntime.cpp
    src/ShellRuntime.h
)

qt_add_qml_module(archie-display
    URI Archie.Observatory
    VERSION 1.0
    QML_FILES
        qml/Main.qml
        qml/LumenScene.qml
        qml/OrbHud.qml
        qml/WorldHud.qml
        qml/WorldBeacons.qml
        qml/WorldImageryMaterial.qml
        qml/Exploration.js
        qml/ForgeView.qml
        qml/OrbViewport.qml
        qml/HologramGrade.qml
)

# Keep relative prototype/font URLs intact; every operational shell asset is local.
file(GLOB_RECURSE ARCHIE_SHELL_QML CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/../shell/prototype/*.qml")
file(GLOB_RECURSE ARCHIE_SHELL_JS CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/../shell/prototype/*.js")
list(FILTER ARCHIE_SHELL_QML EXCLUDE REGEX "Test\\.qml$")
list(FILTER ARCHIE_SHELL_QML EXCLUDE REGEX "/tst_[^/]+\\.qml$")
file(GLOB_RECURSE ARCHIE_SHELL_FONTS CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/../shell/fonts/*.ttf")
qt_add_resources(archie-display "archie-shell-assets"
    PREFIX "/archie/shell"
    BASE "${CMAKE_CURRENT_SOURCE_DIR}/../shell"
    FILES ${ARCHIE_SHELL_QML} ${ARCHIE_SHELL_JS} ${ARCHIE_SHELL_FONTS}
          "${CMAKE_CURRENT_SOURCE_DIR}/../shell/prototype/qmldir")
# Every file in presets/ is a look preset; the display lists them from its resources, so
# adding one is adding the file.
file(GLOB archie_presets RELATIVE "${CMAKE_CURRENT_SOURCE_DIR}" CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/presets/*.json")

qt_add_resources(archie-display "archie-native-assets"
    PREFIX "/archie"
    BASE "${CMAKE_CURRENT_SOURCE_DIR}"
    FILES
        assets/earth-blue-marble.png
        assets/earth-bmng-5400.jpg
        shaders/atmosphere.vert
        shaders/atmosphere.frag
        shaders/star.vert
        shaders/star.frag
        shaders/lumen_orb.vert
        shaders/lumen_orb.frag
        shaders/lumen_sprite.vert
        shaders/lumen_sprite.frag
        shaders/lumen_shell.vert
        shaders/lumen_shell.frag
        shaders/lumen_line.vert
        shaders/lumen_line.frag
        shaders/lumen_ring.vert
        shaders/lumen_ring.frag
        shaders/xavi_surface.vert
        shaders/xavi_surface.frag
        shaders/xavi_ribbon.vert
        shaders/xavi_ribbon.frag
        shaders/lumen_glow.vert
        shaders/lumen_glow.frag
        assets/xavi/prism-structure-4k.png
        assets/xavi/prism-surface-4k.png
        ${archie_presets}
        apps/archie-stage.desktop
        apps/archie-voice.desktop
        apps/archie-editor.desktop
        apps/archie-lienzo.desktop
        apps/archie-cli.desktop
)

# The hologram's grade is a 2D pass over the finished 3D view, as Xavi's CSS filter was
# over his canvas; Qt Quick's ShaderEffect takes it compiled ahead of time.
find_package(Qt6 6.6 REQUIRED COMPONENTS ShaderTools)
qt_add_shaders(archie-display "archie-grade-shaders"
    PREFIX "/archie"
    FILES shaders/hologram_grade.frag)

target_link_libraries(archie-display PRIVATE
    archie::visual
    archie-lumen-design
    archie::osm
    Qt6::Quick
    Qt6::QuickControls2
    Qt6::Quick3D
    Qt6::Network
    Qt6::Concurrent
)

# QML_ELEMENT's generated registration code looks up a QML type's header with an
# angle-bracket include (`#if __has_include(<MeshGeometry.h>)`), which only searches -I
# directories, not "the same directory as the including file" the way a quoted include
# does. Without src/ on the include path that check silently evaluates false and the type
# never gets a definition to register against.
target_include_directories(archie-display PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/src "${ARCHIE_LUMEN_DESIGN_DIR}/include")

target_compile_definitions(archie-display PRIVATE
    QT_NO_CAST_FROM_ASCII
    QT_NO_CAST_TO_ASCII
    QT_NO_URL_CAST_FROM_STRING
)

if(WIN32)
    target_link_libraries(archie-display PRIVATE advapi32)
endif()

install(TARGETS archie-display RUNTIME DESTINATION bin)

option(ARCHIE_NATIVE_BUILD_TESTS "Build native lifecycle and transport tests" OFF)
if(ARCHIE_NATIVE_BUILD_TESTS)
    find_package(Qt6 6.6 REQUIRED COMPONENTS Test)
    enable_testing()
    qt_add_executable(test_native tests/test_native.cpp
        src/LumenField.cpp src/LumenField.h src/LumenGeometry.cpp src/LumenGeometry.h
        src/XaviDesignBridge.cpp src/XaviDesignBridge.h)
    target_include_directories(test_native PRIVATE src "${ARCHIE_LUMEN_DESIGN_DIR}/include")
    target_link_libraries(test_native PRIVATE archie::visual archie-lumen-design Qt6::Quick3D Qt6::Network Qt6::Test)
    qt_add_resources(test_native "archie-test-presets" PREFIX "/archie" BASE "${CMAKE_CURRENT_SOURCE_DIR}"
        FILES ${archie_presets})
    qt_add_resources(test_native "archie-test-hybrid-assets" PREFIX "/archie" BASE "${CMAKE_CURRENT_SOURCE_DIR}"
        FILES assets/xavi/prism-structure-4k.png assets/xavi/prism-surface-4k.png)
    set_source_files_properties(tests/invalid-preset.json PROPERTIES QT_RESOURCE_ALIAS "invalid-test.json")
    qt_add_resources(test_native "archie-invalid-test-preset" PREFIX "/archie/presets" BASE "${CMAKE_CURRENT_SOURCE_DIR}"
        FILES tests/invalid-preset.json)
    # The golden values node computed from the original source (archie-lumen-design).
    target_compile_definitions(test_native PRIVATE ARCHIE_LUMEN_FIXTURES="${ARCHIE_LUMEN_DESIGN_DIR}/fixtures")
    add_test(NAME native_contracts COMMAND test_native)
    qt_add_executable(test_shell_runtime tests/test_shell_runtime.cpp src/ShellRuntime.cpp src/ShellRuntime.h)
    target_include_directories(test_shell_runtime PRIVATE src)
    target_link_libraries(test_shell_runtime PRIVATE Qt6::Core Qt6::Test)
    add_test(NAME shell_runtime_contracts COMMAND test_shell_runtime)
    qt_add_executable(test_exploration_preview tests/test_exploration_preview.cpp src/ExplorationPreview.cpp)
    target_include_directories(test_exploration_preview PRIVATE src)
    target_link_libraries(test_exploration_preview PRIVATE archie::visual Qt6::Gui)
    add_test(NAME exploration_preview_contracts COMMAND test_exploration_preview)
    qt_add_executable(test_globe_stream tests/test_globe_stream.cpp src/GlobeStreamBridge.cpp src/GlobeStreamBridge.h src/GlobeTiles.cpp src/EarthMeshes.cpp src/ExplorationCredential.cpp)
    target_include_directories(test_globe_stream PRIVATE src)
    target_link_libraries(test_globe_stream PRIVATE Qt6::Gui Qt6::Network Qt6::Quick Qt6::Test)
    add_test(NAME globe_stream_contracts COMMAND test_globe_stream)
    set_tests_properties(globe_stream_contracts PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
    qt_add_executable(test_globe_tiles tests/test_globe_tiles.cpp src/GlobeTiles.cpp)
    target_include_directories(test_globe_tiles PRIVATE src)
    target_link_libraries(test_globe_tiles PRIVATE Qt6::Gui)
    add_test(NAME globe_tiles_contracts COMMAND test_globe_tiles)
    qt_add_executable(test_earth_meshes tests/test_earth_meshes.cpp src/EarthMeshes.cpp)
    target_include_directories(test_earth_meshes PRIVATE src)
    target_link_libraries(test_earth_meshes PRIVATE Qt6::Gui)
    target_compile_definitions(test_earth_meshes PRIVATE QT_NO_CAST_FROM_ASCII)
    add_test(NAME earth_mesh_contracts COMMAND test_earth_meshes)
    qt_add_executable(test_world_visuals tests/test_world_visuals.cpp src/WorldVisuals.cpp)
    target_include_directories(test_world_visuals PRIVATE src)
    target_link_libraries(test_world_visuals PRIVATE archie::visual Qt6::Gui)
    add_test(NAME world_visual_contracts COMMAND test_world_visuals)
    qt_add_executable(test_world_texture tests/test_world_texture.cpp src/MeshGeometry.cpp src/MeshGeometry.h src/EarthMeshes.cpp)
    target_include_directories(test_world_texture PRIVATE src)
    target_link_libraries(test_world_texture PRIVATE Qt6::Quick Qt6::Quick3D Qt6::Test)
    qt_add_resources(test_world_texture "world-texture-material" PREFIX "/world-texture" BASE "${CMAKE_CURRENT_SOURCE_DIR}/qml"
        FILES qml/WorldImageryMaterial.qml qml/Exploration.js)
    add_test(NAME world_texture_contracts COMMAND test_world_texture)
    qt_add_executable(test_exploration_credential tests/test_exploration_credential.cpp src/ExplorationCredential.cpp)
    target_include_directories(test_exploration_credential PRIVATE src)
    target_link_libraries(test_exploration_credential PRIVATE Qt6::Core)
    if(WIN32)
        target_link_libraries(test_exploration_credential PRIVATE advapi32)
    endif()
    add_test(NAME exploration_credential_contracts COMMAND test_exploration_credential)
    qt_add_executable(test_world_hud tests/test_world_hud.cpp)
    target_link_libraries(test_world_hud PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Test)
    qt_add_resources(test_world_hud "world-hud-shell" PREFIX "/archie/shell" BASE "${CMAKE_CURRENT_SOURCE_DIR}/../shell"
        FILES ${ARCHIE_SHELL_QML} ${ARCHIE_SHELL_JS} ${ARCHIE_SHELL_FONTS} "${CMAKE_CURRENT_SOURCE_DIR}/../shell/prototype/qmldir")
    qt_add_resources(test_world_hud "world-hud-component" PREFIX "/world-test" BASE "${CMAKE_CURRENT_SOURCE_DIR}/qml"
        FILES qml/WorldHud.qml qml/WorldBeacons.qml qml/Exploration.js)
    add_test(NAME world_hud_contracts COMMAND test_world_hud)
    set_tests_properties(world_hud_contracts PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
    qt_add_executable(test_display_control tests/test_display_control.cpp src/DisplayControl.cpp src/DisplayControl.h
        src/LumenField.cpp src/LumenField.h src/LumenGeometry.cpp src/LumenGeometry.h
        src/XaviDesignBridge.cpp src/XaviDesignBridge.h)
    target_include_directories(test_display_control PRIVATE src "${ARCHIE_LUMEN_DESIGN_DIR}/include")
    target_link_libraries(test_display_control PRIVATE archie::visual archie-lumen-design Qt6::Quick3D Qt6::Network Qt6::Test)
    qt_add_resources(test_display_control "archie-control-test-presets" PREFIX "/archie" BASE "${CMAKE_CURRENT_SOURCE_DIR}"
        FILES ${archie_presets})
    add_test(NAME display_control_contracts COMMAND test_display_control)
    set_target_properties(test_native test_shell_runtime test_display_control PROPERTIES WIN32_EXECUTABLE FALSE)
    option(ARCHIE_NATIVE_BUILD_SHADER_TESTS "Run isolated CPU llvmpipe shader smoke tests" OFF)
    if(ARCHIE_NATIVE_BUILD_SHADER_TESTS)
        file(GLOB ARCHIE_TEST_SHADERS CONFIGURE_DEPENDS RELATIVE "${CMAKE_CURRENT_SOURCE_DIR}"
            "${CMAKE_CURRENT_SOURCE_DIR}/shaders/*.vert" "${CMAKE_CURRENT_SOURCE_DIR}/shaders/*.frag")
        qt_add_executable(test_shaders tests/test_shaders.cpp
            src/LumenField.cpp src/LumenField.h src/LumenGeometry.cpp src/LumenGeometry.h
            src/XaviDesignBridge.cpp src/XaviDesignBridge.h)
        target_include_directories(test_shaders PRIVATE src "${ARCHIE_LUMEN_DESIGN_DIR}/include")
        target_link_libraries(test_shaders PRIVATE archie::visual archie-lumen-design Qt6::Quick3D Qt6::Network Qt6::Test)
        qt_add_resources(test_shaders "archie-shader-test-assets" PREFIX "/archie" BASE "${CMAKE_CURRENT_SOURCE_DIR}"
            FILES ${archie_presets} ${ARCHIE_TEST_SHADERS} qml/LumenScene.qml tests/ShaderSmoke.qml
                assets/xavi/prism-structure-4k.png assets/xavi/prism-surface-4k.png)
        add_test(NAME cpu_shader_contracts COMMAND test_shaders)
        set_tests_properties(cpu_shader_contracts PROPERTIES TIMEOUT 120)
        set_target_properties(test_shaders PROPERTIES WIN32_EXECUTABLE FALSE)
    endif()
endif()
