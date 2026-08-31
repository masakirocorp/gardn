import Foundation
import SwiftUI

@MainActor
final class AgentStore: ObservableObject {
    @Published private(set) var agents: [AgentRecord] = []
    @Published private(set) var connectionMessage: String?
    @Published private(set) var actionError: String?
    @Published private(set) var connected = false
    @Published private(set) var collapsed: Set<AgentRecord.Section>
    @Published private(set) var needsAttention = false
    var onNeedsAttentionChange: ((Bool) -> Void)?


    private var client: GardnClient
    private var timer: Timer?
    private var knownAttention = [String: AgentNotifications.Kind]()
    private var hasBaseline = false

    private static let collapsedKey = "gardn.extra.collapsedSections"

    init(socketPath: String = GardnClient.defaultSocketPath()) {
        client = GardnClient(socketPath: socketPath)
        collapsed = Self.loadCollapsed()
        start()
    }

    func start() {
        if timer != nil {
            refresh()
            return
        }
        refresh()
        timer = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.refresh()
            }
        }
        if let timer {
            RunLoop.main.add(timer, forMode: .common)
        }
    }

    func stop() {
        timer?.invalidate()
        timer = nil
    }

    func refresh() {
        do {
            agents = try client.listAgents()
            connected = true
            connectionMessage = nil
        } catch {
            agents = []
            connected = false
            connectionMessage = error.localizedDescription
        }
        needsAttention = agents.contains { $0.needsAttention }
        onNeedsAttentionChange?(needsAttention)
        publishAttentionChanges()
    }


    func focus(_ agent: AgentRecord) {
        focus(terminalId: agent.terminalId)
    }

    func focus(terminalId: String) {
        do {
            try client.focus(terminalId: terminalId)
            refresh()
        } catch {
            connectionMessage = error.localizedDescription
        }
    }


    func setFollowUp(_ agent: AgentRecord, enabled: Bool) {
        do {
            if enabled {
                try client.addFollowUp(terminalId: agent.terminalId)
            } else {
                try client.removeFollowUp(terminalId: agent.terminalId)
            }
            actionError = nil
            refresh()
        } catch {
            actionError = Self.friendlyError(error)
        }
    }

    private func publishAttentionChanges() {
        var next = [String: AgentNotifications.Kind]()
        for agent in agents {
            guard let kind = AgentNotifications.Kind.of(agent) else { continue }
            next[agent.terminalId] = kind
            if hasBaseline, knownAttention[agent.terminalId] != kind {
                AgentNotifications.post(agent: agent, kind: kind)
            }
        }
        knownAttention = next
        hasBaseline = true
    }

    private static func friendlyError(_ error: Error) -> String {
        let message = error.localizedDescription
        if message.contains("unknown variant") || message.contains("invalid_request") {
            return "Restart Gardn from macos-menu-extra to enable Follow Up."
        }
        return message
    }

    func agents(in section: AgentRecord.Section) -> [AgentRecord] {
        agents.filter { $0.section == section }
    }

    func isCollapsed(_ section: AgentRecord.Section) -> Bool {
        collapsed.contains(section)
    }

    func toggleCollapsed(_ section: AgentRecord.Section) {
        if collapsed.contains(section) {
            collapsed.remove(section)
        } else {
            collapsed.insert(section)
        }
        UserDefaults.standard.set(collapsed.map(\.rawValue), forKey: Self.collapsedKey)
    }

    private static func loadCollapsed() -> Set<AgentRecord.Section> {
        let names = UserDefaults.standard.stringArray(forKey: collapsedKey) ?? []
        return Set(names.compactMap(AgentRecord.Section.init(rawValue:)))
    }
}
