#include "ExplorationCredential.h"
#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QDateTime>
#include <QRegularExpression>
#include <algorithm>
#ifdef Q_OS_UNIX
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>
#endif
#ifdef Q_OS_WIN
#include <QtCore/qt_windows.h>
#include <aclapi.h>
#endif
namespace {
#ifdef Q_OS_WIN
bool ownerPrivate(HANDLE file) {
    HANDLE processToken=nullptr;
    if(!OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY,&processToken)) return false;
    DWORD size=0;GetTokenInformation(processToken,TokenUser,nullptr,0,&size);
    QByteArray tokenUser(static_cast<qsizetype>(size),0);
    const bool identified=size>0 && GetTokenInformation(processToken,TokenUser,tokenUser.data(),size,&size);
    CloseHandle(processToken);
    if(!identified) return false;
    const auto sid=reinterpret_cast<TOKEN_USER*>(tokenUser.data())->User.Sid;
    PSID owner=nullptr;PACL acl=nullptr;PSECURITY_DESCRIPTOR descriptor=nullptr;
    const auto status=GetSecurityInfo(file,SE_FILE_OBJECT,OWNER_SECURITY_INFORMATION|DACL_SECURITY_INFORMATION,
                                     &owner,nullptr,&acl,nullptr,&descriptor);
    if(status!=ERROR_SUCCESS) return false;
    bool privateAcl=owner && EqualSid(owner,sid) && acl && acl->AceCount>0;
    for(DWORD i=0;privateAcl && i<acl->AceCount;++i) {
        void *entry=nullptr;
        if(!GetAce(acl,i,&entry) || static_cast<ACE_HEADER*>(entry)->AceType!=ACCESS_ALLOWED_ACE_TYPE) {privateAcl=false;break;}
        auto *allowed=static_cast<ACCESS_ALLOWED_ACE*>(entry);
        privateAcl=EqualSid(&allowed->SidStart,sid);
    }
    LocalFree(descriptor);return privateAcl;
}
#endif
QByteArray privateCredentialBytes(const QString &path) {
#ifdef Q_OS_UNIX
    const int fd=::open(QFile::encodeName(path).constData(),O_RDONLY|O_NOFOLLOW|O_CLOEXEC|O_NONBLOCK);
    if(fd<0) return {};
    struct stat status{};
    const bool valid=::fstat(fd,&status)==0 && S_ISREG(status.st_mode) && status.st_uid==::geteuid()
        && (status.st_mode&0077)==0 && status.st_size>0 && status.st_size<=512;
    if(!valid) {::close(fd);return {};}
    QFile file;
    if(!file.open(fd,QIODevice::ReadOnly,QFileDevice::AutoCloseHandle)) {::close(fd);return {};}
    const auto bytes=file.read(513);
    return bytes.size()<=512 ? bytes : QByteArray{};
#elif defined(Q_OS_WIN)
    HANDLE file=CreateFileW(reinterpret_cast<LPCWSTR>(path.utf16()),GENERIC_READ|READ_CONTROL,FILE_SHARE_READ,
                            nullptr,OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT,nullptr);
    if(file==INVALID_HANDLE_VALUE) return {};
    BY_HANDLE_FILE_INFORMATION status{};
    const bool valid=GetFileInformationByHandle(file,&status)
        && !(status.dwFileAttributes&(FILE_ATTRIBUTE_REPARSE_POINT|FILE_ATTRIBUTE_DIRECTORY))
        && status.nFileSizeHigh==0 && status.nFileSizeLow>0 && status.nFileSizeLow<=512 && ownerPrivate(file);
    QByteArray bytes(513,0);DWORD read=0;
    const bool received=valid && ReadFile(file,bytes.data(),513,&read,nullptr);
    CloseHandle(file);
    if(!received || read>512) return {};
    bytes.resize(read);return bytes;
#else
    Q_UNUSED(path);return {};
#endif
}
}
QString explorationRuntimeDirectory() {
    if(qEnvironmentVariableIsSet("ARCHIE_WORLD_RUNTIME_DIR")) return qEnvironmentVariable("ARCHIE_WORLD_RUNTIME_DIR");
    if(qEnvironmentVariableIsSet("XDG_RUNTIME_DIR")) return QDir(qEnvironmentVariable("XDG_RUNTIME_DIR")).filePath(QStringLiteral("archie-worldd"));
    if(qEnvironmentVariableIsSet("LOCALAPPDATA")) return QDir(qEnvironmentVariable("LOCALAPPDATA")).filePath(QStringLiteral("ArchieOS/worldd/runtime"));
    if(qEnvironmentVariableIsSet("HOME")) return QDir(qEnvironmentVariable("HOME")).filePath(QStringLiteral(".cache/archie-worldd/runtime"));
    return QStringLiteral("/nonexistent/archie-worldd-runtime-unconfigured");
}
QByteArray explorationProjectionToken(const QString &runtimeDirectory) {
    const auto bytes=privateCredentialBytes(QDir(runtimeDirectory).filePath(QStringLiteral("projection.json")));
    if(bytes.isEmpty()) return {};
    QJsonParseError error;
    const auto document=QJsonDocument::fromJson(bytes,&error);
    if(error.error!=QJsonParseError::NoError || !document.isObject()) return {};
    const auto object=document.object();
    if(object.size()!=2 || object.value(QStringLiteral("schema_version")).toDouble()!=1 || !object.value(QStringLiteral("token")).isString()) return {};
    const auto token=object.value(QStringLiteral("token")).toString().toLatin1();
    if(token.size()!=64 || !std::all_of(token.begin(),token.end(),[](char c){return (c>='0'&&c<='9')||(c>='a'&&c<='f')||(c>='A'&&c<='F');})) return {};
    return token;
}

QVariantMap worldNavigationSession(const QString &runtimeDirectory) {
    const auto bytes=privateCredentialBytes(QDir(runtimeDirectory).filePath(QStringLiteral("navigation.json")));
    const auto document=QJsonDocument::fromJson(bytes);
    if(!document.isObject())return {};
    const auto object=document.object();
    auto hex=[](const QString &text,int length){return text.size()==length&&std::all_of(text.begin(),text.end(),[](QChar c){return (c.unicode()>='0'&&c.unicode()<='9')||(c.unicode()>='a'&&c.unicode()<='f')||(c.unicode()>='A'&&c.unicode()<='F');});};
    const auto address=object.value(QStringLiteral("address")).toString();
    const auto match=QRegularExpression(QStringLiteral("^127\\.0\\.0\\.1:([0-9]{1,5})$")).match(address);
    if(object.size()!=6||object.value(QStringLiteral("schema_version")).toInt()!=1||!match.hasMatch()||match.captured(1).toUInt()==0||match.captured(1).toUInt()>65535||
       !hex(object.value(QStringLiteral("token")).toString(),64)||!hex(object.value(QStringLiteral("session_id")).toString(),32)||
       (object.value(QStringLiteral("domain"))!=QStringLiteral("personal")&&object.value(QStringLiteral("domain"))!=QStringLiteral("work"))||object.value(QStringLiteral("expires_at_ms")).toDouble()<=QDateTime::currentMSecsSinceEpoch())return {};
    return object.toVariantMap();
}
