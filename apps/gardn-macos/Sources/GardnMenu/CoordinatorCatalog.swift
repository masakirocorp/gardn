import Foundation

struct ExtraCoordinator: Identifiable, Hashable, Codable, Sendable {
    enum Kind: String, Codable, Sendable {
        case local
        case remote
    }

    var id: String
    var kind: Kind
    var name: String
    var running: Bool
    var socketPath: String?
    var target: String?
    var session: String?

    var title: String {
        kind == .local && session == "default" ? "Default" : name
    }

    var subtitle: String {
        switch kind {
        case .local:
            return running ? "Local coordinator · running" : "Local coordinator · stopped"
        case .remote:
            return "Remote coordinator · \(target ?? name)"
        }
    }
}

struct ExtraRemoteRecord: Codable, Hashable, Identifiable, Sendable {
    var id: String
    var target: String
    var session: String
    var name: String
}

@MainActor
final class CoordinatorCatalog: ObservableObject {
    @Published private(set) var coordinators: [ExtraCoordinator] = []
    @Published var selectedId: String
    @Published var addError: String?
    @Published private(set) var isRefreshingLocals = false

    private var locals: [ExtraCoordinator]
    private var remotes: [ExtraRemoteRecord]
    private var connectProcess: Process?
    private let localLoader: () async -> [ExtraCoordinator]?
    private let persistSelection: (String) -> Void
    private var refreshGeneration = 0

    private static let selectedKey = "gardn.extra.selectedCoordinator"
    private static let remotesKey = "gardn.extra.remoteCoordinators"

    init(
        localLoader: (() async -> [ExtraCoordinator]?)? = nil,
        remoteRecords: [ExtraRemoteRecord]? = nil,
        selectedId: String? = nil,
        persistSelection: ((String) -> Void)? = nil
    ) {
        self.localLoader =
            localLoader
            ?? {
                await Task.detached {
                    Self.loadLocalCoordinators()
                }.value
            }
        self.persistSelection =
            persistSelection
            ?? { id in UserDefaults.standard.set(id, forKey: Self.selectedKey) }
        locals = [Self.fallbackLocal]
        remotes = remoteRecords ?? Self.loadRemotes()
        self.selectedId =
            selectedId
            ?? UserDefaults.standard.string(forKey: Self.selectedKey)
            ?? "local:default"
        rebuildCatalog(validateSelection: false)
    }

    var selected: ExtraCoordinator? {
        coordinators.first { $0.id == selectedId } ?? coordinators.first
    }

    func refreshLocals() async {
        let generation = beginLocalRefresh()
        _ = await finishLocalRefresh(generation)
    }

    func beginLocalRefresh() -> Int {
        refreshGeneration += 1
        isRefreshingLocals = true
        return refreshGeneration
    }

    func finishLocalRefresh(_ generation: Int) async -> Bool {
        let loadedLocals = await localLoader()
        guard generation == refreshGeneration else { return false }
        isRefreshingLocals = false
        guard let loadedLocals else { return false }
        locals = loadedLocals
        rebuildCatalog()
        return true
    }

    func select(_ id: String) {
        selectedId = id
        persistSelection(id)
    }

    func addRemote(target: String, session: String) -> ExtraCoordinator? {
        let trimmedTarget = target.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedSession = session.trimmingCharacters(in: .whitespacesAndNewlines)
        let sessionName = trimmedSession.isEmpty ? "default" : trimmedSession
        guard !trimmedTarget.isEmpty else {
            addError = "SSH target is required"
            return nil
        }
        let id = "remote:\(trimmedTarget):\(sessionName)"
        if remotes.contains(where: { $0.id == id }) {
            addError = "That Gardn instance is already saved"
            return nil
        }
        let name = sessionName == "default" ? trimmedTarget : "\(trimmedTarget) (\(sessionName))"
        let record = ExtraRemoteRecord(id: id, target: trimmedTarget, session: sessionName, name: name)
        remotes.append(record)
        Self.saveRemotes(remotes)
        addError = nil
        rebuildCatalog()
        select(id)
        return coordinators.first { $0.id == id }
    }

    func removeRemote(_ id: String) {
        remotes.removeAll { $0.id == id }
        Self.saveRemotes(remotes)
        rebuildCatalog()
    }

    private func rebuildCatalog(validateSelection: Bool = true) {
        coordinators =
            locals
            + remotes.map { remote in
                ExtraCoordinator(
                    id: remote.id,
                    kind: .remote,
                    name: remote.name,
                    running: false,
                    socketPath: nil,
                    target: remote.target,
                    session: remote.session
                )
            }
        if validateSelection,
           coordinators.contains(where: { $0.id == selectedId }) == false
        {
            select(coordinators.first?.id ?? "local:default")
        }
    }

    func socketPath(for coordinator: ExtraCoordinator) throws -> String {
        switch coordinator.kind {
        case .local:
            guard let path = coordinator.socketPath, !path.isEmpty else {
                throw GardnClientError(message: "No local API socket for \(coordinator.name)")
            }
            return path
        case .remote:
            return try connectRemote(coordinator)
        }
    }

    func stopConnectProcess() {
        connectProcess?.terminate()
        connectProcess = nil
    }

    private func connectRemote(_ coordinator: ExtraCoordinator) throws -> String {
        guard let target = coordinator.target else {
            throw GardnClientError(message: "Remote Gardn instance is missing an SSH target")
        }
        stopConnectProcess()
        var arguments = ["extra", "connect", "--remote", target, "--json"]
        if let session = coordinator.session, session != "default" {
            arguments.append(contentsOf: ["--session", session])
        }
        let process = try BundledGardn.process(arguments: arguments)
        let stdout = Pipe()
        process.standardOutput = stdout
        process.standardError = Pipe()
        try process.run()
        connectProcess = process
        let handle = stdout.fileHandleForReading
        var data = Data()
        let deadline = Date().addingTimeInterval(20)
        while Date() < deadline {
            let chunk = handle.availableData
            if !chunk.isEmpty {
                data.append(chunk)
                if data.contains(UInt8(ascii: "\n")) {
                    break
                }
            }
            if !process.isRunning, data.isEmpty {
                throw GardnClientError(message: "Could not reach \(coordinator.name)")
            }
            Thread.sleep(forTimeInterval: 0.05)
        }
        guard let line = String(data: data, encoding: .utf8)?
            .split(separator: "\n", maxSplits: 1)
            .first
            .map(String.init),
            let payload = line.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: payload) as? [String: Any],
            let socketPath = json["socket_path"] as? String
        else {
            throw GardnClientError(message: "Could not open \(coordinator.name)")
        }
        return socketPath
    }

    nonisolated private static func loadLocalCoordinators() -> [ExtraCoordinator]? {
        let process: Process
        do {
            process = try BundledGardn.process(arguments: ["extra", "list", "--json"])
        } catch {
            BundledGardn.logFailure(error)
            return nil
        }

        let stdout = Pipe()
        process.standardOutput = stdout
        process.standardError = FileHandle.nullDevice
        process.standardInput = FileHandle.nullDevice
        do {
            try process.run()
            let timeout = DispatchWorkItem {
                if process.isRunning { process.terminate() }
            }
            DispatchQueue.global(qos: .utility).asyncAfter(
                deadline: .now() + .seconds(2),
                execute: timeout
            )
            defer { timeout.cancel() }
            let data = stdout.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            guard process.terminationReason == .exit, process.terminationStatus == 0,
                  let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let rows = json["coordinators"] as? [[String: Any]]
            else {
                return nil
            }
            let locals = rows.compactMap { row -> ExtraCoordinator? in
                guard let id = row["id"] as? String else { return nil }
                let session = row["session"] as? String ?? row["name"] as? String ?? "default"
                let jsonName = row["name"] as? String ?? session
                return ExtraCoordinator(
                    id: id,
                    kind: .local,
                    name: localDisplayName(session: session, jsonName: jsonName),
                    running: row["running"] as? Bool ?? false,
                    socketPath: row["socket_path"] as? String,
                    target: nil,
                    session: session
                )
            }
            return locals.isEmpty ? [fallbackLocal] : locals
        } catch {
            return nil
        }
    }

    nonisolated private static func localDisplayName(session: String, jsonName: String) -> String {
        if session == "default" || jsonName.isEmpty || jsonName == "default" {
            return thisMacName
        }
        return jsonName
    }

    nonisolated private static var thisMacName: String {
        let name = Host.current().localizedName?.trimmingCharacters(in: .whitespacesAndNewlines)
        if let name, !name.isEmpty { return name }
        return "This Mac"
    }

    nonisolated private static var fallbackLocal: ExtraCoordinator {
        ExtraCoordinator(
            id: "local:default",
            kind: .local,
            name: thisMacName,
            running: false,
            socketPath: GardnClient.defaultSocketPath(),
            target: nil,
            session: "default"
        )
    }

    private static func loadRemotes() -> [ExtraRemoteRecord] {
        guard let data = UserDefaults.standard.data(forKey: remotesKey),
              let remotes = try? JSONDecoder().decode([ExtraRemoteRecord].self, from: data)
        else {
            return []
        }
        return remotes
    }

    private static func saveRemotes(_ remotes: [ExtraRemoteRecord]) {
        if let data = try? JSONEncoder().encode(remotes) {
            UserDefaults.standard.set(data, forKey: remotesKey)
        }
    }
}
