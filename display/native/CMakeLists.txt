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

qt_add_executable(archie-display
    src/main.cpp
    src/ArchieBridge.cpp
    src/ArchieBridge.h
    src/MeshGeometry.cpp
    src/MeshGeometry.h
    src/Payload.cpp
    src/Payload.h
    src/LumenField.cpp
    src/LumenField.h
    src/LumenGeometry.cpp
    src/LumenGeometry.h
    src/AppCatalog.cpp
    src/AppCatalog.h
    src/DisplayControl.cpp
    src/DisplayControl.h
)

qt_add_qml_module(archie-display
    URI Archie.Observatory
    VERSION 1.0
    QML_FILES
        qml/Main.qml
        qml/LumenScene.qml
        qml/OrbHud.qml
        qml/ForgeView.qml
)

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
        ${archie_presets}
        apps/archie-stage.desktop
        apps/archie-voice.desktop
        apps/archie-editor.desktop
        apps/archie-lienzo.desktop
        apps/archie-cli.desktop
)

target_link_libraries(archie-display PRIVATE
    archie::visual
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
target_include_directories(archie-display PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/src)

target_compile_definitions(archie-display PRIVATE
    QT_NO_CAST_FROM_ASCII
    QT_NO_CAST_TO_ASCII
    QT_NO_URL_CAST_FROM_STRING
)

install(TARGETS archie-display RUNTIME DESTINATION bin)
