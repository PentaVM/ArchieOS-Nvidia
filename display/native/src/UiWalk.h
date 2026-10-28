#pragma once

// archie-display --ui-walk <steps.json> --ui-walk-out <dir>: drives the shell with real mouse and
// key events, one step at a time, and records what each click changed. For each click it saves a
// screenshot, the shell's own state before and after (Main.qml uiState()) and how much the
// clicked panel's pixels moved. A click that changes neither is reported as "no_effect": a
// control that looks like a button and does nothing. Development and verification only; it
// sends events to its own window and nothing else.
//
// Steps (a JSON array):
//   {"shot": "name"}                     save the window
//   {"click": "objectName"}              left click at the item's centre ("dclick", "rclick" too)
//   {"key": "Escape"} / {"key": "Ctrl+K"}  a key press on the window
//   {"run": "action.id"}                 Main.run(id), for setting up state between clicks
//   {"wait": 800}
//   {"clickAll": "objectName", "reset": [steps]}  click every visible, enabled MouseArea inside
//                                         the item, running the reset steps before each one
//   {"label": "text"}                    a line in the report

#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QGuiApplication>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QKeyEvent>
#include <QKeySequence>
#include <QMouseEvent>
#include <QWheelEvent>
#include <QPointer>
#include <QQuickItem>
#include <QQuickWindow>
#include <QRegularExpression>
#include <QTimer>

#include <cmath>
#include <cstdlib>
#include <functional>
#include <utility>

class UiWalk final : public QObject {
public:
    UiWalk(QQuickWindow *window, const QString &script, const QString &out, QObject *parent = nullptr)
        : QObject(parent), m_window(window), m_out(out) {
        QFile file(script);
        if (file.open(QIODevice::ReadOnly)) {
            const auto steps = QJsonDocument::fromJson(file.readAll()).array();
            for (const auto &s : steps) m_queue.append(s.toObject());
        }
        QDir().mkpath(m_out);
        m_report.setFileName(m_out + QStringLiteral("/report.jsonl"));
        m_report.open(QIODevice::WriteOnly | QIODevice::Truncate | QIODevice::Text);
    }
    bool ready() const { return !m_queue.isEmpty() && m_report.isOpen(); }
    void setCaptureAllowed(std::function<bool()> allowed) { m_captureAllowed=std::move(allowed); }
    void start(int settleMs) { QTimer::singleShot(settleMs, this, [this] { next(); }); }

private:
    QPointer<QQuickWindow> m_window;
    QString m_out;
    QFile m_report;
    QList<QJsonObject> m_queue;
    int m_index = 0;
    int m_clicks = 0;
    int m_noEffect = 0;
    std::function<bool()> m_captureAllowed;

    // The visual tree, not the QObject one: a Repeater's delegates are its parent's child
    // items but not necessarily its QObject children.
    static void collect(QQuickItem *item, QList<QQuickItem *> &out) {
        for (auto *child : item->childItems()) { out.append(child); collect(child, out); }
    }
    static QList<QQuickItem *> items(QQuickItem *root) { QList<QQuickItem *> out; collect(root, out); return out; }

    QQuickItem *desktop() const {
        for (auto *item : items(m_window->contentItem()))
            if (item->objectName() == QLatin1String("desktop")) return item;
        return nullptr;
    }

    QString state() const {
        QVariant value;
        QObject *surface=desktop();
        if(!surface)surface=m_window;
        if(surface)QMetaObject::invokeMethod(surface, "uiState", Q_RETURN_ARG(QVariant, value));
        return value.toString();
    }

    static bool usable(QQuickItem *item) {
        return item && item->isVisible() && item->isEnabled() && item->width() > 2 && item->height() > 2;
    }

    QQuickItem *find(const QString &name) const {
        for (auto *item : items(m_window->contentItem()))
            if (item->objectName() == name && usable(item)) return item;
        return nullptr;
    }

    QList<QQuickItem *> mouseAreas(QQuickItem *container) const {
        QList<QQuickItem *> out;
        const QRectF bounds(QPointF(0, 0), m_window->size());
        for (auto *item : items(container)) {
            if (!usable(item) || QString::fromLatin1(item->metaObject()->className()) != QLatin1String("QQuickMouseArea"))
                continue;
            const QPointF c = item->mapToScene(QPointF(item->width() / 2, item->height() / 2));
            if (bounds.contains(c)) out.append(item);
        }
        return out;
    }

    // A readable name for an unnamed control: the nearest objectName up the tree, and the text
    // of the first Text found in its parent.
    static QString describe(QQuickItem *item) {
        QString name;
        for (QQuickItem *p = item; p && name.isEmpty(); p = p->parentItem()) name = p->objectName();
        QString text;
        std::function<void(QQuickItem *)> walk = [&](QQuickItem *n) {
            if (!text.isEmpty() || !n) return;
            const QVariant t = n->property("text");
            if (t.isValid() && !t.toString().trimmed().isEmpty() && n != item) { text = t.toString().trimmed(); return; }
            for (auto *c : n->childItems()) walk(c);
        };
        walk(item->parentItem());
        return text.isEmpty() ? name : name + QStringLiteral(" · ") + text.left(60);
    }

    QImage grab() const {
        if(m_captureAllowed && !m_captureAllowed()) {
            qWarning("archie-display: online-preview imagery cannot be captured or exported");
            return {};
        }
        return m_window->grabWindow();
    }

    static double moved(const QImage &a, const QImage &b, const QRectF &r) {
        if (a.isNull() || b.isNull()) return 0;
        const QRect rect = r.toAlignedRect().intersected(a.rect());
        if (rect.isEmpty()) return 0;
        const QImage x = a.copy(rect).scaled(96, 54).convertToFormat(QImage::Format_Grayscale8);
        const QImage y = b.copy(rect).scaled(96, 54).convertToFormat(QImage::Format_Grayscale8);
        double sum = 0;
        for (int row = 0; row < x.height(); ++row) {
            const uchar *p = x.constScanLine(row), *q = y.constScanLine(row);
            for (int col = 0; col < x.width(); ++col) sum += std::abs(int(p[col]) - int(q[col]));
        }
        return sum / (x.width() * x.height());
    }

    static QStringList changedKeys(const QString &before, const QString &after) {
        QStringList keys;
        const auto a = QJsonDocument::fromJson(before.toUtf8()).object();
        const auto b = QJsonDocument::fromJson(after.toUtf8()).object();
        for (auto it = b.begin(); it != b.end(); ++it)
            if (a.value(it.key()) != it.value()) keys << it.key();
        return keys;
    }

    QString save(const QImage &image, const QString &name) {
        if(image.isNull()) return {};
        const QString file = QStringLiteral("%1-%2.png").arg(m_index, 3, 10, QLatin1Char('0')).arg(name);
        image.save(m_out + QLatin1Char('/') + file);
        return file;
    }

    void log(QJsonObject line) {
        line.insert(QStringLiteral("i"), m_index);
        m_report.write(QJsonDocument(line).toJson(QJsonDocument::Compact) + '\n');
        m_report.flush();
    }

    // Sent straight to the window, not through the platform's input path. Under an open menu,
    // an entry drawn over a window's title bar did nothing here while the same click through
    // QTest worked (tst_MenuOverWindows.qml): a no_effect is a lead to check, not a verdict.
    void mouse(const QPointF &p, Qt::MouseButton button, bool twice) {
        const QPointF global = m_window->mapToGlobal(p);
        auto send = [&](QEvent::Type type, Qt::MouseButtons held) {
            QMouseEvent event(type, p, global, type == QEvent::MouseMove ? Qt::NoButton : button, held, Qt::NoModifier);
            QGuiApplication::sendEvent(m_window, &event);
        };
        send(QEvent::MouseMove, Qt::NoButton);
        send(QEvent::MouseButtonPress, button);
        send(QEvent::MouseButtonRelease, Qt::NoButton);
        if (twice) {
            send(QEvent::MouseButtonDblClick, button);
            send(QEvent::MouseButtonRelease, Qt::NoButton);
        }
    }

    void key(const QString &text) {
        const QKeySequence sequence(text);
        if (sequence.isEmpty()) return;
        const QKeyCombination combination = sequence[0];
        QKeyEvent press(QEvent::KeyPress, combination.key(), combination.keyboardModifiers());
        QKeyEvent release(QEvent::KeyRelease, combination.key(), combination.keyboardModifiers());
        QGuiApplication::sendEvent(m_window, &press);
        QGuiApplication::sendEvent(m_window, &release);
    }

    // One click with its evidence. `container` bounds the pixel comparison.
    void clickAndRecord(QQuickItem *target, const QString &label, Qt::MouseButton button, bool twice,
                        QQuickItem *container, int settle) {
        const QString before = state();
        const QImage shotBefore = grab();
        const QRectF region = (container ? container : target)->mapRectToScene(
            QRectF(0, 0, (container ? container : target)->width(), (container ? container : target)->height()));
        const QPointF centre = target->mapToScene(QPointF(target->width() / 2, target->height() / 2));
        mouse(centre, button, twice);
        QTimer::singleShot(settle, this, [this, before, shotBefore, region, label, centre] {
            const QString after = state();
            const QImage shotAfter = grab();
            const QStringList keys = changedKeys(before, after);
            const double pixels = moved(shotBefore, shotAfter, region);
            const bool effect = !keys.isEmpty() || pixels >= 2.0;
            ++m_clicks;
            if (!effect) ++m_noEffect;
            log({{QStringLiteral("kind"), QStringLiteral("click")}, {QStringLiteral("target"), label},
                 {QStringLiteral("x"), centre.x()}, {QStringLiteral("y"), centre.y()},
                 {QStringLiteral("changed"), QJsonArray::fromStringList(keys)},
                 {QStringLiteral("pixels"), std::round(pixels * 10) / 10},
                 {QStringLiteral("result"), effect ? QStringLiteral("ok") : QStringLiteral("no_effect")},
                 {QStringLiteral("shot"), save(shotAfter, QString(label).replace(QRegularExpression(QStringLiteral("[^A-Za-z0-9._-]+")), QStringLiteral("_")).left(60))},
                 {QStringLiteral("state"), QJsonDocument::fromJson(after.toUtf8()).object()}});
            ++m_index;
            next();
        });
    }

    void finish() {
        log({{QStringLiteral("kind"), QStringLiteral("summary")}, {QStringLiteral("clicks"), m_clicks},
             {QStringLiteral("no_effect"), m_noEffect}});
        m_report.close();
        qInfo("archie-display ui-walk: %d clicks, %d without effect; report %s/report.jsonl",
              m_clicks, m_noEffect, qPrintable(m_out));
        QGuiApplication::exit(m_noEffect == 0 ? 0 : 3);
    }

    void next() {
        if (!m_window) return;
        if (m_queue.isEmpty()) { finish(); return; }
        const QJsonObject step = m_queue.takeFirst();
        qInfo("archie-display ui-walk: step %d %s", m_index,
              QJsonDocument(step).toJson(QJsonDocument::Compact).left(160).constData());
        const int settle = step.value(QStringLiteral("settle")).toInt(650);
        auto later = [this](int ms) { QTimer::singleShot(ms, this, [this] { ++m_index; next(); }); };

        if (step.contains(QStringLiteral("wait"))) { later(step.value(QStringLiteral("wait")).toInt()); return; }
        if (step.contains(QStringLiteral("label"))) {
            log({{QStringLiteral("kind"), QStringLiteral("label")}, {QStringLiteral("text"), step.value(QStringLiteral("label"))}});
            later(0); return;
        }
        if (step.contains(QStringLiteral("state"))) {
            log({{QStringLiteral("kind"),QStringLiteral("state")},{QStringLiteral("label"),step.value(QStringLiteral("state"))},
                 {QStringLiteral("state"),QJsonDocument::fromJson(state().toUtf8()).object()}});
            later(0);return;
        }
        if(step.contains(QStringLiteral("wheel")) || step.contains(QStringLiteral("drag"))){
            const QPointF point(m_window->width()*step.value(QStringLiteral("x")).toDouble(0.65),m_window->height()*step.value(QStringLiteral("y")).toDouble(0.5));
            const auto before=state();
            if(step.contains(QStringLiteral("wheel"))){
                const int ticks=std::clamp(step.value(QStringLiteral("wheel")).toInt(),-10,10);
                QWheelEvent event(point,m_window->mapToGlobal(point),QPoint(),QPoint(0,ticks*120),Qt::NoButton,Qt::NoModifier,Qt::NoScrollPhase,false);
                QGuiApplication::sendEvent(m_window,&event);
            }else{
                const auto delta=step.value(QStringLiteral("drag")).toArray();
                const QPointF finish=point+QPointF(delta.at(0).toDouble(),delta.at(1).toDouble());
                QMouseEvent press(QEvent::MouseButtonPress,point,m_window->mapToGlobal(point),Qt::LeftButton,Qt::LeftButton,Qt::NoModifier);
                QMouseEvent move(QEvent::MouseMove,finish,m_window->mapToGlobal(finish),Qt::NoButton,Qt::LeftButton,Qt::NoModifier);
                QMouseEvent release(QEvent::MouseButtonRelease,finish,m_window->mapToGlobal(finish),Qt::LeftButton,Qt::NoButton,Qt::NoModifier);
                QGuiApplication::sendEvent(m_window,&press);QGuiApplication::sendEvent(m_window,&move);QGuiApplication::sendEvent(m_window,&release);
            }
            QTimer::singleShot(settle,this,[this,before,step]{const auto after=state();const auto changed=changedKeys(before,after);log({{QStringLiteral("kind"),step.contains(QStringLiteral("wheel"))?QStringLiteral("wheel"):QStringLiteral("drag")},{QStringLiteral("changed"),QJsonArray::fromStringList(changed)},{QStringLiteral("state"),QJsonDocument::fromJson(after.toUtf8()).object()}});++m_index;next();});
            return;
        }
        if (step.contains(QStringLiteral("shot"))) {
            log({{QStringLiteral("kind"), QStringLiteral("shot")}, {QStringLiteral("shot"), save(grab(), step.value(QStringLiteral("shot")).toString())},
                 {QStringLiteral("state"), QJsonDocument::fromJson(state().toUtf8()).object()}});
            later(0); return;
        }
        if (step.contains(QStringLiteral("key"))) { key(step.value(QStringLiteral("key")).toString()); later(settle); return; }
        if (step.contains(QStringLiteral("run"))) {
            QVariant ok;
            if (auto *d = desktop())
                QMetaObject::invokeMethod(d, "run", Q_RETURN_ARG(QVariant, ok), Q_ARG(QVariant, step.value(QStringLiteral("run")).toString()));
            later(settle); return;
        }
        for (const auto &kind : {QStringLiteral("click"), QStringLiteral("dclick"), QStringLiteral("rclick")}) {
            if (!step.contains(kind)) continue;
            const QString name = step.value(kind).toString();
            QQuickItem *target = find(name);
            if (!target) {
                log({{QStringLiteral("kind"), QStringLiteral("click")}, {QStringLiteral("target"), name},
                     {QStringLiteral("result"), QStringLiteral("not_found")}});
                ++m_clicks; ++m_noEffect;
                later(0); return;
            }
            QQuickItem *container = nullptr;
            for (QQuickItem *p = target->parentItem(); p && !container; p = p->parentItem())
                if (p->objectName().startsWith(QStringLiteral("win.")) || p->objectName().startsWith(QStringLiteral("panel.")))
                    container = p;
            clickAndRecord(target, name, kind == QLatin1String("rclick") ? Qt::RightButton : Qt::LeftButton,
                           kind == QLatin1String("dclick"), container, settle);
            return;
        }
        if (step.contains(QStringLiteral("clickAll"))) {
            // Expand now, click later: each control is found again after its own reset.
            const QString name = step.value(QStringLiteral("clickAll")).toString();
            QQuickItem *container = find(name);
            const int count = container ? mouseAreas(container).size() : 0;
            log({{QStringLiteral("kind"), QStringLiteral("clickAll")}, {QStringLiteral("container"), name},
                 {QStringLiteral("controls"), count}});
            QList<QJsonObject> expanded;
            for (int n = 0; n < count; ++n) {
                for (const auto &r : step.value(QStringLiteral("reset")).toArray()) expanded.append(r.toObject());
                expanded.append({{QStringLiteral("clickNth"), name}, {QStringLiteral("n"), n}, {QStringLiteral("settle"), settle}});
            }
            m_queue = expanded + m_queue;
            later(0); return;
        }
        if (step.contains(QStringLiteral("clickNth"))) {
            const QString name = step.value(QStringLiteral("clickNth")).toString();
            const int n = step.value(QStringLiteral("n")).toInt();
            QQuickItem *container = find(name);
            const auto areas = container ? mouseAreas(container) : QList<QQuickItem *>{};
            if (n >= areas.size()) {
                log({{QStringLiteral("kind"), QStringLiteral("click")}, {QStringLiteral("target"), QStringLiteral("%1#%2").arg(name).arg(n)},
                     {QStringLiteral("result"), QStringLiteral("not_found")}});
                later(0); return;
            }
            clickAndRecord(areas.at(n), QStringLiteral("%1#%2 %3").arg(name).arg(n).arg(describe(areas.at(n))),
                           Qt::LeftButton, false, container, settle);
            return;
        }
        log({{QStringLiteral("kind"), QStringLiteral("unknown_step")}, {QStringLiteral("step"), step}});
        later(0);
    }
};
