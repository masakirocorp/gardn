import Foundation
import UserNotifications

enum AgentNotifications {
    static let terminalIdKey = "terminalId"

    static func requestAuthorization() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { _, _ in }
    }

    static func post(agent: AgentRecord, kind: Kind) {
        let content = UNMutableNotificationContent()
        content.title = agent.title
        content.body = kind.body
        content.sound = .default
        content.userInfo = [terminalIdKey: agent.terminalId]
        content.threadIdentifier = agent.terminalId
        let request = UNNotificationRequest(
            identifier: agent.terminalId,
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request)
    }

    enum Kind: Equatable {
        case blocked
        case done
        case followUp
        case triage

        var body: String {
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
