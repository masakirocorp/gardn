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

    static func present(
        request: PresentationRequest,
        receipt: @escaping @Sendable (PresentationOutcome) -> Void
    ) {
        let notification = request.notification
        let complete: (PresentationOutcome) -> Void = { outcome in
            receipt(outcome)
        }

        if notification.visual == .none {
            guard notification.sound != .none else {
                complete(.rejected("empty_presentation"))
                return
            }
            DispatchQueue.main.async {
                NSSound.beep()
                complete(.submitted)
            }
            return
        }
        guard notification.visual == .system else {
            complete(.rejected("unsupported_visual"))
            return
        }

        lock.lock()
        let allowed = isAuthorized
        lock.unlock()
        guard allowed else {
            complete(.rejected("not_authorized"))
            return
        }
        let content = UNMutableNotificationContent()
        content.title = notification.title
        content.body = notification.body ?? ""
        content.sound = notification.sound == .none ? nil : .default
        let notificationId: [String: Any] = [
            "coordinator_epoch": notification.id.coordinatorEpoch,
            "sequence": notification.id.sequence,
        ]
        var userInfo: [String: Any] = [
            "notification_id": notificationId,
        ]
        if let target = notification.target {
            userInfo[terminalIdKey] = target.terminalId
            content.threadIdentifier = target.terminalId
        }
        content.userInfo = userInfo
        let identifier = "gardn-\(notification.id.coordinatorEpoch)-\(notification.id.sequence)"
        let request = UNNotificationRequest(identifier: identifier, content: content, trigger: nil)
        let center = UNUserNotificationCenter.current()
        center.add(request) { error in
            if let error {
                log.error("notification post failed: \(error.localizedDescription, privacy: .public)")
                complete(.rejected(error.localizedDescription))
            } else {
                complete(.submitted)
            }
        }
    }
}
