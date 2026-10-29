#pragma once

#include <QString>

// The language the person reads the shell in: ARCHIE_UI_LANG=en for English, Spanish
// otherwise. The QML side reads the same variable through Qt.uiLanguage (Lang.js).
inline bool uiEnglish()
{
    static const bool english = qEnvironmentVariable("ARCHIE_UI_LANG").startsWith(QLatin1String("en"));
    return english;
}

// A sentence the shell shows, in the reader's language.
inline QString uiText(const QString &spanish, const QString &english)
{
    return uiEnglish() ? english : spanish;
}
