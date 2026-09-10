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
    private var runtimePoll: DispatchSourceTimer?
    private var runtimeProbeGeneration = 0
    private var runtimeTask: Task<Void, Never>?
    @Published private(set) var runtimeNotice: RuntimeNotice?
    private var presenterStream: GardnPresenterStream?
    private var presenterTask: Task<Void, Never>?


    private static let collapsedKey = "gardn.extra.collapsedSections"

    init(socketPath: String = GardnClient.defaultSocketPath()) {
        client = GardnClient(socketPath: socketPath)
        collapsed = Self.loadCollapsed()
        reconnectToSelected()
    }

    func start() {
        if poll != nil {
            refresh()
            refreshRuntimeStatus()
            return
        }
        refresh()
        refreshRuntimeStatus()
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

        let runtimePoll = DispatchSource.makeTimerSource(queue: .main)
        runtimePoll.schedule(
            deadline: .now() + .seconds(10),
            repeating: .seconds(10),
            leeway: .seconds(1)
        )
        runtimePoll.setEventHandler { [weak self] in
            self?.refreshRuntimeStatus()
        }
        runtimePoll.resume()
        self.runtimePoll = runtimePoll
        presenterTask?.cancel()
        presenterTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                if self.presenterStream?.active != true {
                    self.presenterStream = self.client.startPresenterStream { request, receipt in
                        AgentNotifications.present(request: request, receipt: receipt)
                    }
                }
                try? await Task.sleep(for: .seconds(2))
            }
        }
    }
    func stop() {
        presenterStream = nil
        presenterTask?.cancel()
        presenterTask = nil
        runtimeProbeGeneration += 1
        poll?.cancel()
        poll = nil
        runtimePoll?.cancel()
        runtimePoll = nil
        runtimeTask?.cancel()
        runtimeTask = nil
        catalog.stopConnectProcess()
    }

    func selectCoordinator(_ id: String) {
        catalog.select(id)
        runtimeNotice = .unknown
        reconnectToSelected()
        presenterStream = nil
        refresh()
        refreshRuntimeStatus()
    }


    func addRemoteCoordinator(target: String, session: String) {
        if catalog.addRemote(target: target, session: session) != nil {
            runtimeNotice = .unknown
            reconnectToSelected()
            refresh()
            refreshRuntimeStatus()
        }
    }

    func openSettings() {
        onOpenSettings?()
    }

    func refreshRuntimeStatus() {
        runtimeProbeGeneration += 1
        let generation = runtimeProbeGeneration
        let socketPath = client.socketPath
        let appVersion = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
        runtimeTask?.cancel()
        runtimeTask = Task { [weak self] in
            do {
                let runtime = try await BundledGardn.runtimeStatus(socketPath: socketPath)
                guard !Task.isCancelled else { return }
                let installation = GardnInstallation.compare(
                    appVersion: appVersion,
                    cliVersion: runtime.clientVersion
                )
                let notice = RuntimeNotice.presentation(runtime: runtime, installation: installation)
                guard let self, self.runtimeProbeGeneration == generation else { return }
                self.runtimeNotice = notice
            } catch {
                guard let self, self.runtimeProbeGeneration == generation else { return }
                self.runtimeNotice = .unknown
            }
        }
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
            client = GardnClient(socketPath: "")
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



    private static func friendlyError(_ error: Error) -> String {
        let message = error.localizedDescription
        if message.contains("unknown variant") || message.contains("invalid_request") {
            return "Restart Gardn to enable Follow Up."
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
