#include "MeshGeometry.h"
#include "WorldImageProvider.h"
#include "EarthMeshes.h"
#include <QBuffer>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QTest>
#include <QVector3D>
#include <cstdlib>

void check(bool ok,const char *why){if(!ok){qCritical("%s",why);std::exit(1);}}
int main(int argc,char **argv) {
    QGuiApplication app(argc,argv);
    qmlRegisterType<MeshGeometry>("WorldTest",1,0,"MeshGeometry");
    QQmlApplicationEngine engine;
    auto *provider=new WorldImageProvider;
    QImage image(64,64,QImage::Format_ARGB32);
    for(int y=0;y<64;++y) for(int x=0;x<64;++x) image.setPixelColor(x,y,x<16?QColor(0,0,0,0):x<32?QColor(128,128,128):y<32?Qt::red:Qt::blue);
    provider->replace({{QStringLiteral("orientation"),image}});
    engine.addImageProvider(QStringLiteral("world"),provider);
    // Geography uses image v=0 at north. Verify that convention through the
    // production material, actual custom geometry, image provider and Qt renderer.
    const float vertices[]={
        -50,50,0, 0,0,1, 0,0, -50,-50,0, 0,0,1, 0,1, 50,50,0, 0,0,1, 1,0,
        50,50,0, 0,0,1, 1,0, -50,-50,0, 0,0,1, 0,1, 50,-50,0, 0,0,1, 1,1};
    const quint32 indices[]={0,1,2,3,4,5};
    engine.rootContext()->setContextProperty(QStringLiteral("tileVertices"),QByteArray(reinterpret_cast<const char*>(vertices),sizeof(vertices)));
    engine.rootContext()->setContextProperty(QStringLiteral("tileIndices"),QByteArray(reinterpret_cast<const char*>(indices),sizeof(indices)));
    engine.loadData(R"(
        import QtQuick
        import QtQuick3D
        import WorldTest
        import "qrc:/world-texture"
        Window {
            width:256;height:256;visible:true
            View3D {
                anchors.fill:parent
                camera:tileCamera
                environment:SceneEnvironment {backgroundMode:SceneEnvironment.Color;clearColor:"black";tonemapMode:SceneEnvironment.TonemapModeLinear}
                OrthographicCamera {id:tileCamera;z:200;horizontalMagnification:1;verticalMagnification:1}
                Model {
                    geometry:MeshGeometry {vertices:tileVertices;triangleIndices:tileIndices;hasUv:true}
                    materials:WorldImageryMaterial {textureSource:"image://world/orientation";terrainColor:"#00ff00"}
                }
            }
        }
    )");
    check(!engine.rootObjects().isEmpty(),"production imagery material loads");
    auto *window=qobject_cast<QQuickWindow*>(engine.rootObjects().first());
    check(window&&QTest::qWaitForWindowExposed(window),"texture render surface is exposed");
    QTest::qWait(500);
    const auto frame=window->grabWindow();
    check(!frame.isNull(),"actual Qt texture frame is available");
    const auto north=frame.pixelColor(frame.width()/2,frame.height()/2-25);
    const auto south=frame.pixelColor(frame.width()/2,frame.height()/2+25);
    qInfo()<<"north/south texture pixels"<<north<<south;
    check(north.red()>200&&north.blue()<30&&south.blue()>200&&south.red()<30,"image north row maps to geographical north and south row maps to south");
    const auto missing=frame.pixelColor(frame.width()/2-37,frame.height()/2);
    check(missing.green()>200&&missing.red()<30&&missing.blue()<30,"transparent image gaps show neutral terrain material on the same surface");
    const auto gray=frame.pixelColor(frame.width()/2-12,frame.height()/2);
    qInfo()<<"neutral terrain/gray pixels"<<missing<<gray;
    check(std::abs(gray.red()-128)<10&&std::abs(gray.green()-128)<10&&std::abs(gray.blue()-128)<10,"imagery midtones retain source color without cinematic tone mapping");
    window->close();
    // A real Earth-centred perspective camera must see nearby imagery too.
    // The old near plane used distance to Earth's centre and clipped away
    // the entire ground at street altitude despite successful tile delivery.
    const float groundVertices[]={
        -.00015F,.00015F,20, 0,0,1, 0,0, -.00015F,-.00015F,20, 0,0,1, 0,1, .00015F,.00015F,20, 0,0,1, 1,0,
        .00015F,.00015F,20, 0,0,1, 1,0, -.00015F,-.00015F,20, 0,0,1, 0,1, .00015F,-.00015F,20, 0,0,1, 1,1};
    engine.rootContext()->setContextProperty(QStringLiteral("groundVertices"),QByteArray(reinterpret_cast<const char*>(groundVertices),sizeof(groundVertices)));
    engine.loadData(R"(
        import QtQuick
        import QtQuick3D
        import WorldTest
        import "qrc:/world-texture"
        import "qrc:/world-texture/Exploration.js" as Exploration
        Window {
            width:256;height:256;visible:true
            View3D {
                anchors.fill:parent
                camera:groundCamera
                environment:SceneEnvironment {backgroundMode:SceneEnvironment.Color;clearColor:"black";tonemapMode:SceneEnvironment.TonemapModeLinear}
                PerspectiveCamera {
                    id:groundCamera; z:20+149*20/6371008.8;fieldOfView:45;clipFar:1600
                    clipNear:Exploration.worldNearClip({cameraEye:{x:0,y:0,z:z},cameraTarget:{x:0,y:0,z:0}})
                }
                Model {
                    geometry:MeshGeometry {vertices:groundVertices;triangleIndices:tileIndices;hasUv:true}
                    materials:WorldImageryMaterial {textureSource:"image://world/orientation";terrainColor:"#00ff00"}
                }
            }
        }
    )");
    auto *groundWindow=qobject_cast<QQuickWindow*>(engine.rootObjects().last());
    check(groundWindow&&groundWindow!=window&&QTest::qWaitForWindowExposed(groundWindow),"street-altitude perspective fixture is exposed");
    QTest::qWait(500);
    const auto ground=groundWindow->grabWindow();
    const auto detail=ground.pixelColor(ground.width()/2,ground.height()/2-25);
    qInfo()<<"149m ground texture pixel"<<detail;
    check(detail.red()>160&&detail.red()>3*detail.blue(),"actual perspective rendering retains detail 149m above the globe");
    groundWindow->close();
    // Synthetic original JPEG + native local 3D wall. Its nonzero radial
    // height must remain visible from an inclined Earth-centred camera.
    QImage wallImage(256,256,QImage::Format_RGB32);wallImage.fill(Qt::red);QByteArray jpeg;QBuffer jpegBuffer(&jpeg);jpegBuffer.open(QIODevice::WriteOnly);wallImage.save(&jpegBuffer,"JPEG");
    const float wall[]={-.0002F,0,0,0,-1,0,0,1, .0002F,0,0,0,-1,0,1,1, -.0002F,0,.0003F,0,-1,0,0,0, .0002F,0,.0003F,0,-1,0,1,0};
    const quint32 wallIndices[]={0,1,2,2,1,3};
    const auto wallKey=QStringLiteral("earth:synthetic-wall");
    EarthMeshes earth;earth.ingest({QVariantMap{{QStringLiteral("key"),wallKey},{QStringLiteral("origin"),QVariantList{0,0,20}},{QStringLiteral("vertex_count"),4},{QStringLiteral("triangle_count"),2},{QStringLiteral("vertices_base64"),QString::fromLatin1(QByteArray(reinterpret_cast<const char*>(wall),sizeof(wall)).toBase64())},{QStringLiteral("indices_base64"),QString::fromLatin1(QByteArray(reinterpret_cast<const char*>(wallIndices),sizeof(wallIndices)).toBase64())},{QStringLiteral("texture_base64"),QString::fromLatin1(jpeg.toBase64())}}},{wallKey});
    check(earth.meshes().size()==1,"bounded photogrammetry decoder accepts synthetic raised wall");
    auto *earthProvider=new WorldImageProvider;earthProvider->replace(earth.images());engine.addImageProvider(QStringLiteral("globe"),earthProvider);
    engine.rootContext()->setContextProperty(QStringLiteral("earthMesh"),earth.meshes().first());
    engine.loadData(R"(
        import QtQuick
        import QtQuick3D
        import WorldTest
        import "qrc:/world-texture"
        Window {
            width:256;height:256;visible:true
            View3D {
                anchors.fill:parent;camera:inclined
                environment:SceneEnvironment {backgroundMode:SceneEnvironment.Color;clearColor:"black";tonemapMode:SceneEnvironment.TonemapModeLinear}
                PerspectiveCamera {id:inclined;y:-.001;z:20.001;fieldOfView:45;clipNear:.000001;clipFar:1;Component.onCompleted:lookAt(Qt.vector3d(0,0,20.00015))}
                Model {
                    position:earthMesh.origin
                    geometry:MeshGeometry {vertices:earthMesh.vertices;triangleIndices:earthMesh.triangleIndices;hasUv:true}
                    materials:WorldImageryMaterial {textureSource:earthMesh.textureSource;terrainColor:"#00ff00"}
                }
            }
        }
    )");
    auto *earthWindow=qobject_cast<QQuickWindow*>(engine.rootObjects().last());check(earthWindow&&earthWindow!=groundWindow&&QTest::qWaitForWindowExposed(earthWindow),"inclined 3D mesh render surface exposed");QTest::qWait(500);
    const auto raised=earthWindow->grabWindow();int redPixels=0;for(int y=0;y<raised.height();++y)for(int x=0;x<raised.width();++x){const auto color=raised.pixelColor(x,y);if(color.red()>160&&color.green()<40&&color.blue()<40)++redPixels;}
    check(redPixels>1000,"original JPEG covers nonzero-height mesh under inclined camera; flattened wall would have zero area");
    qInfo("World image-provider texture render contract passed");
}
