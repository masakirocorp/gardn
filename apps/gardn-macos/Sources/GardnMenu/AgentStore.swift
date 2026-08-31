import Foundation
import SwiftUI

@MainActor
final class AgentStore: ObservableObject {
    @Published private(set) var agents: [AgentRecord] = []
    @Published private(set) var connectionMessage: String?
    @Published private(set) var connected = false

    private var client: GardnClient
    private var timer: Timer?

    var needsAttention: Bool {
        agents.contains { $0.needsAttention }
    }

    init(socketPath: String = GardnClient.defaultSocketPath()) {
        client = GardnClient(socketPath: socketPath)
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
    }

    func focus(_ agent: AgentRecord) {
        do {
            try client.focus(terminalId: agent.terminalId)
            refresh()
        } catch {
            connectionMessage = error.localizedDescription
        }
    }

    func agents(in section: AgentRecord.Section) -> [AgentRecord] {
        agents.filter { $0.section == section }
    }
}
