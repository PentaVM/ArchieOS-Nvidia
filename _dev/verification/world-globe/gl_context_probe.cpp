#include <QGuiApplication>
#include <QOffscreenSurface>
#include <QOpenGLContext>
#include <QOpenGLFunctions>
#include <QDebug>
int main(int argc,char **argv){
    QGuiApplication app(argc,argv);QOffscreenSurface surface;surface.create();
    QOpenGLContext context;if(!context.create()||!context.makeCurrent(&surface))return 2;
    auto *gl=context.functions();
    qInfo().noquote()<<"vendor:"<<reinterpret_cast<const char*>(gl->glGetString(GL_VENDOR));
    qInfo().noquote()<<"renderer:"<<reinterpret_cast<const char*>(gl->glGetString(GL_RENDERER));
    qInfo().noquote()<<"version:"<<reinterpret_cast<const char*>(gl->glGetString(GL_VERSION));
}
