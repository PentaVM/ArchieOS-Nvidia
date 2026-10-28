#include "ExplorationCredential.h"
#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QFile>
#include <QTemporaryDir>
#include <cstdlib>
#ifdef Q_OS_UNIX
#include <unistd.h>
#include <sys/stat.h>
#endif
void check(bool value,const char *message){if(!value){qCritical("%s",message);std::exit(1);}}
void write(const QString &path,const QByteArray &content){QFile f(path);check(f.open(QIODevice::WriteOnly|QIODevice::Truncate),"create fixture");check(f.write(content)==content.size(),"write fixture");f.close();check(f.setPermissions(QFileDevice::ReadOwner|QFileDevice::WriteOwner),"private fixture permissions");}
int main(int argc,char **argv) {
    QCoreApplication app(argc,argv);
    const char *names[]={"ARCHIE_WORLD_RUNTIME_DIR","XDG_RUNTIME_DIR","LOCALAPPDATA","HOME"};
    QByteArray original[4];bool present[4];
    for(int i=0;i<4;++i){present[i]=qEnvironmentVariableIsSet(names[i]);original[i]=qgetenv(names[i]);qunsetenv(names[i]);}
    check(explorationRuntimeDirectory()==QStringLiteral("/nonexistent/archie-worldd-runtime-unconfigured"),"no public temporary fallback");
    qputenv("HOME","/owner");check(explorationRuntimeDirectory()==QStringLiteral("/owner/.cache/archie-worldd/runtime"),"HOME fallback");
    qputenv("LOCALAPPDATA","/local");check(explorationRuntimeDirectory()==QStringLiteral("/local/ArchieOS/worldd/runtime"),"LOCALAPPDATA precedence");
    qputenv("XDG_RUNTIME_DIR","/runtime");check(explorationRuntimeDirectory()==QStringLiteral("/runtime/archie-worldd"),"XDG precedence");
    qputenv("ARCHIE_WORLD_RUNTIME_DIR","/explicit");check(explorationRuntimeDirectory()==QStringLiteral("/explicit"),"explicit override precedence");
    for(int i=0;i<4;++i){if(present[i])qputenv(names[i],original[i]);else qunsetenv(names[i]);}
#ifdef Q_OS_UNIX
    QTemporaryDir dir;check(dir.isValid(),"temporary fixture directory");
    const auto path=QDir(dir.path()).filePath(QStringLiteral("projection.json"));
    const auto command=QDir(dir.path()).filePath(QStringLiteral("control.json"));
    const QByteArray token(64,'a');const auto credential=QByteArray("{\"schema_version\":1,\"token\":\"")+token+"\"}";
    write(command,credential);check(explorationProjectionToken(dir.path()).isEmpty(),"command token is never a fallback");
    write(path,credential);check(explorationProjectionToken(dir.path())==token,"separate read-only projection token accepted");
    QFile::setPermissions(path,QFileDevice::ReadOwner|QFileDevice::WriteOwner|QFileDevice::ReadGroup);
    check(explorationProjectionToken(dir.path()).isEmpty(),"group-readable credential refused");
    write(path,QByteArray(513,'x'));check(explorationProjectionToken(dir.path()).isEmpty(),"oversized credential refused");
    write(path,"{\"schema_version\":2,\"token\":\"bad\"}");check(explorationProjectionToken(dir.path()).isEmpty(),"schema and token shape validated");
    write(path,QByteArray("{\"schema_version\":1,\"token\":\"")+QByteArray(64,'g')+"\"}");check(explorationProjectionToken(dir.path()).isEmpty(),"only hex tokens accepted");
    write(path,credential.left(credential.size()-1)+",\"extra\":true}");check(explorationProjectionToken(dir.path()).isEmpty(),"unexpected descriptor fields refused");
    check(QFile::remove(path),"remove fixture before symlink");
    check(::symlink(QFile::encodeName(command).constData(),QFile::encodeName(path).constData())==0,"create fixture symlink");
    check(explorationProjectionToken(dir.path()).isEmpty(),"symlink credential refused");check(QFile::remove(path),"remove fixture symlink");
    check(::mkfifo(QFile::encodeName(path).constData(),0600)==0,"create fixture nonregular file");
    check(explorationProjectionToken(dir.path()).isEmpty(),"nonregular credential refused without blocking");
    qInfo("World projection credential contracts passed (Unix owner/mode/no-follow)");
#else
    qInfo("World projection path contracts passed; Windows ACL behavior requires native Windows validation");
#endif
}
