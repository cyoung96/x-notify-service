#!/usr/bin/env python3
"""模拟 org.freedesktop.Notifications 守护进程:占住总线名,把收到的 Notify
调用逐条写成 JSON Lines,供 CI 断言系统通知兜底路径真实送达。
用法: mock-notify-daemon.py <输出文件.jsonl>
依赖: python3-dbus + python3-gi(Debian apt 可装)"""

import json
import sys

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

OUT = sys.argv[1]
NOTIFY = "org.freedesktop.Notifications"


class Notifications(dbus.service.Object):
    def __init__(self, bus: dbus.Bus):
        super().__init__(bus, "/org/freedesktop/Notifications")

    @dbus.service.method(NOTIFY, in_signature="susssasa{sv}i", out_signature="u")
    def Notify(self, app_name, _replaces, _icon, summary, body, _actions, _hints, _timeout):
        with open(OUT, "a", encoding="utf-8") as f:
            f.write(
                json.dumps(
                    {"app": str(app_name), "summary": str(summary), "body": str(body)},
                    ensure_ascii=False,
                )
                + "\n"
            )
        print(f"captured: {summary}", flush=True)
        return 1

    @dbus.service.method(NOTIFY, in_signature="u", out_signature="")
    def CloseNotification(self, _id):
        pass

    @dbus.service.method(NOTIFY, in_signature="", out_signature="as")
    def GetCapabilities(self):
        return ["body"]

    @dbus.service.method(NOTIFY, in_signature="", out_signature="ssss")
    def GetServerInformation(self):
        return ("mock", "ci", "1.0", "1.2")


def main() -> None:
    DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    # BusName 必须持有引用,否则被 GC 后总线名立即释放(python-dbus 经典坑)
    name = dbus.service.BusName(NOTIFY, bus)
    Notifications(bus)
    print(f"owning {name.get_name()}", flush=True)
    print("mock notification daemon ready", flush=True)
    GLib.MainLoop().run()


if __name__ == "__main__":
    main()
