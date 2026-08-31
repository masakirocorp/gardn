import Foundation
import os
import UserNotifications

enum AgentNotifications {
    static let terminalIdKey = "terminalId"
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

    static func post(agent: AgentRecord, kind: Kind) {
        lock.lock()
        let allowed = isAuthorized
        lock.unlock()
        guard allowed else { return }
        let content = UNMutableNotificationContent()
        content.title = agent.title
        content.subtitle = kind.headline
        let details = detailLine(agent)
        content.body = details.isEmpty ? kind.headline : details
        content.sound = .default
        content.userInfo = [terminalIdKey: agent.terminalId]
        content.threadIdentifier = agent.terminalId
        let request = UNNotificationRequest(
            identifier: agent.terminalId,
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request) { error in
            if let error {
                log.error("notification post failed: \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    private static func detailLine(_ agent: AgentRecord) -> String {
        var parts: [String] = []
        if let group = agent.groupName?.trimmingCharacters(in: .whitespacesAndNewlines), !group.isEmpty {
            parts.append(group)
        }
        if let status = agent.statusLabel, !status.isEmpty {
            parts.append(status)
        }
        if let age = agent.age, !age.isEmpty {
            parts.append(age)
        }
        return parts.joined(separator: " · ")
    }

    enum Kind: Equatable {
        case blocked
        case done
        case followUp
        case triage

        var headline: String {
            switch self {
            case .blocked: return "Needs attention"
            case .done: return "Finished"
            case .followUp: return "Follow Up"
            case .triage: return "Triage"
            }
        }


        static func of(_ agent: AgentRecord) -> Kind? {
            if agent.status == .blocked { return .blocked }
            if agent.status == .done { return .done }
            if agent.followUp { return .followUp }
            if agent.inTriage { return .triage }
            return nil
        }
    }
}
