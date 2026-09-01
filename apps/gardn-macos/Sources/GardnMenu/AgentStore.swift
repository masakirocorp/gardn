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
    let catalog = CoordinatorCatalog()
    var onNeedsAttentionChange: ((Bool) -> Void)?
    var onDidFocus: (() -> Void)?
    var onOpenSettings: (() -> Void)?


    private var client: GardnClient
    private var poll: DispatchSourceTimer?
    private var knownAttention = [String: AgentNotifications.Kind]()
    private var hasBaseline = false

    private static let collapsedKey = "gardn.extra.collapsedSections"

    init(socketPath: String = GardnClient.defaultSocketPath()) {
        client = GardnClient(socketPath: socketPath)
        collapsed = Self.loadCollapsed()
        reconnectToSelected()
    }

    func start() {
        if poll != nil {
            refresh()
            return
        }
        refresh()
        let poll = DispatchSource.makeTimerSource(queue: .main)
        poll.schedule(
            deadline: .now() + .seconds(2),
            repeating: .seconds(2),
            leeway: .milliseconds(200)
        )
        poll.setEventHandler { [weak self] in
            self?.refresh()
        }
        poll.resume()
        self.poll = poll
    }

    func stop() {
        poll?.cancel()
        poll = nil
        catalog.stopConnectProcess()
    }

    func selectCoordinator(_ id: String) {
        catalog.select(id)
        reconnectToSelected()
        refresh()
    }

    func addRemoteCoordinator(target: String, session: String) {
        if catalog.addRemote(target: target, session: session) != nil {
            reconnectToSelected()
            refresh()
        }
    }

    func openSettings() {
        onOpenSettings?()
    }

    private func reconnectToSelected() {
        catalog.refreshLocals()
        guard let selected = catalog.selected else {
            client = GardnClient(socketPath: GardnClient.defaultSocketPath())
            return
        }
        do {
            let path = try catalog.socketPath(for: selected)
            client = GardnClient(socketPath: path)
        } catch {
            connectionMessage = error.localizedDescription
            connected = false
            agents = []
        }
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
            onDidFocus?()
            HostTerminal.raise(
                apiSocketPath: client.socketPath,
                coordinator: catalog.selected
            )
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
        guard connected else { return }
        var next = [String: AgentNotifications.Kind]()
        var seen = Set<String>()
        for agent in agents {
            guard let kind = AgentNotifications.Kind.of(agent) else { continue }
            guard seen.insert(agent.terminalId).inserted else { continue }
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
