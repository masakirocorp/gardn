import AppKit
import Foundation
import os
import UserNotifications

enum AgentNotifications {
    static let terminalIdKey = "terminal_id"
    private static let log = Logger(subsystem: "com.masakiro.gardn.menu", category: "notifications")
    private static let lock = NSLock()
    private static var isAuthorized = false

    static func requestAuthorization() {
        let center = UNUserNotificationCenter.current()
        center.getNotificationSettings { settings in
            switch settings.authorizationStatus {
            case .notDetermined:
                center.requestAuthorization(options: [.alert, .sound]) { granted, error in
                    if let error {
                        log.error("notification authorization failed: \(error.localizedDescription, privacy: .public)")
                    }
                    lock.lock()
                    isAuthorized = granted
                    lock.unlock()
                }
            case .authorized, .provisional:
                lock.lock()
                isAuthorized = true
                lock.unlock()
            default:
                lock.lock()
                isAuthorized = false
                lock.unlock()
                log.error("notifications not allowed; enable Gardn in System Settings > Notifications")
            }
        }
    }

    static func present(request: [String: Any], receipt: @escaping ([String: Any]) -> Void) {
        let registration = request["registration_id"] as? [String: Any]
        let notification = request["notification"] as? [String: Any]
        let registrationId = registration ?? [:]
        let notificationId = notification?["id"] as? [String: Any] ?? [:]
        let baseReceipt: (Any) -> Void = { outcome in
            receipt([
                "registration_id": registrationId,
                "notification_id": notificationId,
                "outcome": outcome,
            ])
        }
        guard let notification else {
            baseReceipt(["rejected": "missing_notification"])
            return
        }

        let visual = notification["visual"] as? String ?? "none"
        let sound = notification["sound"] as? String ?? "none"
        if visual == "none" {
            guard sound != "none" else {
                baseReceipt(["rejected": "empty_presentation"])
                return
            }
            DispatchQueue.main.async {
                NSSound.beep()
                baseReceipt("submitted")
            }
            return
        }
        guard visual == "system" else {
            baseReceipt(["rejected": "unsupported_visual"])
            return
        }

        lock.lock()
        let allowed = isAuthorized
        lock.unlock()
        guard allowed else {
            baseReceipt(["rejected": "not_authorized"])
            return
        }
        let content = UNMutableNotificationContent()
        content.title = notification["title"] as? String ?? "Gardn"
        content.body = notification["body"] as? String ?? ""
        content.sound = sound == "none" ? nil : .default
        var userInfo: [String: Any] = [
            "notification_id": notificationId,
        ]
        if let target = notification["target"] as? [String: Any], let terminalId = target["terminal_id"] as? String {
            userInfo[terminalIdKey] = terminalId
            content.threadIdentifier = terminalId
        }
        content.userInfo = userInfo
        let identifier = "gardn-\(notificationId["coordinator_epoch"] as? String ?? "epoch")-\(notificationId["sequence"] as? NSNumber ?? 0)"
        let request = UNNotificationRequest(identifier: identifier, content: content, trigger: nil)
        let center = UNUserNotificationCenter.current()
        center.add(request) { error in
            if let error {
                log.error("notification post failed: \(error.localizedDescription, privacy: .public)")
                baseReceipt(["rejected": error.localizedDescription])
            } else {
                baseReceipt("submitted")
            }
        }
    }
}
