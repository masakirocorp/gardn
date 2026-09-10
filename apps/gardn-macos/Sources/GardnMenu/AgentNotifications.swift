import Foundation
import os
import UserNotifications

enum AgentNotifications {
    static let terminalIdKey = "terminal_id"
    private static let log = Logger(subsystem: "com.masakiro.gardn.menu", category: "notifications")
    private static let lock = NSLock()
    private static var isAuthorized = false

    private enum EffectOutcome: Sendable {
        case notRequested
        case succeeded
        case failed(String)
    }

    static func requestAuthorization() {
        let center = UNUserNotificationCenter.current()
        center.getNotificationSettings { settings in
            switch settings.authorizationStatus {
            case .notDetermined:
                center.requestAuthorization(options: [.alert]) { granted, error in
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
        Task.detached(priority: .userInitiated) {
            async let visual = visualOutcome(for: notification)
            async let sound = soundOutcome(for: notification.sound)
            let (visualOutcome, soundOutcome) = await (visual, sound)
            receipt(combine(visual: visualOutcome, sound: soundOutcome))
        }
    }

    private static func visualOutcome(for notification: StateNotification) async -> EffectOutcome {
        switch notification.visual {
        case .none:
            return .notRequested
        case .gardn, .terminal:
            return .failed("unsupported_visual")
        case .system:
            break
        }

        guard notificationsAllowed() else { return .failed("not_authorized") }

        let content = UNMutableNotificationContent()
        content.title = notification.title
        content.body = notification.body ?? ""
        content.sound = nil
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

        return await withCheckedContinuation { continuation in
            UNUserNotificationCenter.current().add(request) { error in
                if let error {
                    log.error("notification post failed: \(error.localizedDescription, privacy: .public)")
                    continuation.resume(returning: .failed(error.localizedDescription))
                } else {
                    continuation.resume(returning: .succeeded)
                }
            }
        }
    }

    private static func soundOutcome(for sound: NotificationSound) async -> EffectOutcome {
        guard sound != .none else { return .notRequested }
        switch await BundledGardn.playSound(sound) {
        case .played:
            return .succeeded
        case .suppressed:
            return .failed("sound_suppressed")
        case .failed(let reason):
            log.error("sound playback failed: \(reason, privacy: .public)")
            return .failed(reason)
        }
    }

    private static func notificationsAllowed() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return isAuthorized
    }

    private static func combine(
        visual: EffectOutcome,
        sound: EffectOutcome
    ) -> PresentationOutcome {
        if case .succeeded = visual {
            return .submitted
        }
        if case .succeeded = sound {
            return .submitted
        }
        if case .failed(let reason) = visual {
            return .rejected(reason)
        }
        if case .failed(let reason) = sound {
            return .rejected(reason)
        }
        return .rejected("empty_presentation")
    }
}
