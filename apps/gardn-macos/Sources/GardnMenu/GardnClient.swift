import Darwin
import Foundation

struct GardnClientError: Error, LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

struct AgentRecord: Identifiable, Hashable {
    enum Status: String {
        case idle
        case working
        case blocked
        case done
        case unknown
    }

    enum Section: String, CaseIterable, Hashable {
        case triage = "Triage"
        case followUp = "Follow Up"
        case working = "Working"
        case idle = "Idle"
    }

    var id: String { terminalId }
    var terminalId: String
    var title: String
    var groupName: String?
    var groupAccent: String?
    var status: Status
    var statusLabel: String?
    var age: String?
    var followUp: Bool
    var inTriage: Bool
    var focused: Bool

    var section: Section {
        if followUp { return .followUp }
        if inTriage || status == .blocked || status == .done { return .triage }
        switch status {
        case .working: return .working
        case .idle, .unknown, .blocked, .done: return .idle
        }
    }

    var needsAttention: Bool {
        followUp || inTriage || status == .blocked || status == .done
    }


    var showsStatus: Bool {
        section == .triage || section == .followUp
    }
}

struct GardnClient {
    var socketPath: String

    static func defaultSocketPath() -> String {
        if let override = ProcessInfo.processInfo.environment["GARDN_SOCKET_PATH"], !override.isEmpty {
            return override
        }
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let production = "\(home)/.config/gardn/gardn.sock"
        if FileManager.default.fileExists(atPath: production) {
            return production
        }
        return "\(home)/.config/gardn-dev/gardn.sock"
    }


    func listAgents() throws -> [AgentRecord] {
        let agents = try resultObject(transact([
            "id": "menu:agent.list",
            "method": "agent.list",
            "params": [:],
        ]))
        let workspaces = (try? resultObject(transact([
            "id": "menu:workspace.list",
            "method": "workspace.list",
            "params": [:],
        ]))) ?? [:]
        let groups = (try? resultObject(transact([
            "id": "menu:group.list",
            "method": "group.list",
            "params": [:],
        ]))) ?? [:]
        let tabs = (try? resultObject(transact([
            "id": "menu:tab.list",
            "method": "tab.list",
            "params": [:],
        ]))) ?? [:]
        return Self.assemble(
            agents: agents["agents"] as? [[String: Any]] ?? [],
            workspaces: workspaces["workspaces"] as? [[String: Any]] ?? [],
            groups: groups["groups"] as? [[String: Any]] ?? [],
            tabs: tabs["tabs"] as? [[String: Any]] ?? []
        )

    }

    func focus(terminalId: String) throws {
        _ = try transact([
            "id": "menu:agent.focus",
            "method": "agent.focus",
            "params": ["target": terminalId],
        ])
    }

    func addFollowUp(terminalId: String) throws {
        _ = try transact([
            "id": "menu:agent.follow_up.add",
            "method": "agent.follow_up.add",
            "params": ["target": terminalId],
        ])
    }

    func removeFollowUp(terminalId: String) throws {
        _ = try transact([
            "id": "menu:agent.follow_up.remove",
            "method": "agent.follow_up.remove",
            "params": ["target": terminalId],
        ])
    }

    private func resultObject(_ json: [String: Any]) throws -> [String: Any] {
        if let error = json["error"] as? [String: Any] {
            throw GardnClientError(message: error["message"] as? String ?? "request failed")
        }
        guard let result = json["result"] as? [String: Any] else {
            throw GardnClientError(message: "missing result")
        }
        return result
    }

    private func transact(_ body: [String: Any]) throws -> [String: Any] {
        let payload = try JSONSerialization.data(withJSONObject: body)
        var line = payload
        line.append(contentsOf: [0x0A])
        let reply = try unixRequest(path: socketPath, payload: line)
        guard let json = try JSONSerialization.jsonObject(with: reply) as? [String: Any] else {
            throw GardnClientError(message: "invalid JSON")
        }
        return json
    }

    @MainActor
    func startPresenterStream(onRequest: @escaping GardnPresenterStream.RequestHandler) -> GardnPresenterStream? {
        let stream = GardnPresenterStream(
            socketPath: socketPath,
            requestHandler: onRequest
        )
        guard stream.start() else { return nil }
        return stream
    }



    private static func assemble(
        agents: [[String: Any]],
        workspaces: [[String: Any]],
        groups: [[String: Any]],
        tabs: [[String: Any]]
    ) -> [AgentRecord] {
        var workspaceById: [String: [String: Any]] = [:]
        for workspace in workspaces {
            if let id = workspace["workspace_id"] as? String {
                workspaceById[id] = workspace
            }
        }
        var groupById: [String: [String: Any]] = [:]
        for group in groups {
            if let id = group["group_id"] as? String {
                groupById[id] = group
            }
        }
        var tabById: [String: [String: Any]] = [:]
        for tab in tabs {
            if let id = tab["tab_id"] as? String {
                tabById[id] = tab
            }
        }

        return agents.compactMap { raw -> AgentRecord? in
            guard let terminalId = raw["terminal_id"] as? String else { return nil }
            let followUp = boolValue(raw["follow_up"])
            let status = AgentRecord.Status(rawValue: raw["agent_status"] as? String ?? "unknown") ?? .unknown
            if !followUp, status == .unknown, raw["agent"] == nil, raw["display_agent"] == nil, raw["name"] == nil {
                return nil
            }
            let workspaceId = raw["workspace_id"] as? String
            let workspace = workspaceId.flatMap { workspaceById[$0] }
            let tab = (raw["tab_id"] as? String).flatMap { tabById[$0] }
            let groupId = workspace?["group_id"] as? String
            let group = groupId.flatMap { groupById[$0] }
            return AgentRecord(
                terminalId: terminalId,
                title: sidebarTitle(workspace: workspace, tab: tab, raw: raw, fallback: terminalId),
                groupName: group?["name"] as? String,
                groupAccent: group?["accent"] as? String,
                status: status,
                statusLabel: statusText(status),
                age: activityAge(
                    unixSecs: unixSecs(raw["follow_up_added_at_unix_secs"])
                        ?? unixSecs(raw["last_meaningful_agent_activity_unix_secs"])
                ),
                followUp: followUp,
                inTriage: boolValue(raw["in_triage"]) || status == .blocked || status == .done,
                focused: boolValue(raw["focused"])
            )
        }
    }


    private static func sidebarTitle(
        workspace: [String: Any]?,
        tab: [String: Any]?,
        raw: [String: Any],
        fallback: String
    ) -> String {
        var title = (workspace?["label"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
        if title == nil || title?.isEmpty == true {
            let cwd = (raw["foreground_cwd"] as? String) ?? (raw["cwd"] as? String)
            title = cwd.flatMap { URL(fileURLWithPath: $0).lastPathComponent }
                ?? (raw["name"] as? String)
                ?? fallback
        }
        let tabCount = intValue(workspace?["tab_count"]) ?? 1
        if tabCount > 1, let tabLabel = tab?["label"] as? String, isUsefulTabLabel(tabLabel) {
            title = "\(title!)/\(tabLabel)"
        }
        let paneCount = intValue(tab?["pane_count"]) ?? 1
        if paneCount > 1, let paneLabel = raw["name"] as? String, !paneLabel.isEmpty {
            title = "\(title!)/\(paneLabel)"
        }
        return title ?? fallback
    }

    private static func isUsefulTabLabel(_ label: String) -> Bool {
        let trimmed = label.trimmingCharacters(in: .whitespacesAndNewlines)
        return !trimmed.isEmpty && trimmed.contains(where: { !$0.isNumber })
    }

    private static func statusText(_ status: AgentRecord.Status) -> String {
        switch status {
        case .blocked: return "Blocked"
        case .working: return "Working"
        case .done: return "Done"
        case .idle, .unknown: return "Idle"
        }
    }
}


private func boolValue(_ value: Any?) -> Bool {
    if let value = value as? Bool { return value }
    if let value = value as? NSNumber { return value.boolValue }
    return false
}

private func intValue(_ value: Any?) -> Int? {
    if let value = value as? Int { return value }
    if let value = value as? NSNumber { return value.intValue }
    return nil
}

private func unixSecs(_ value: Any?) -> UInt64? {
    if let n = value as? NSNumber { return n.uint64Value }
    if let n = value as? UInt64 { return n }
    if let n = value as? Int, n >= 0 { return UInt64(n) }
    return nil
}

private func activityAge(unixSecs: UInt64?) -> String? {
    guard let unixSecs else { return nil }
    let now = UInt64(Date().timeIntervalSince1970)
    let elapsed = now > unixSecs ? now - unixSecs : 0
    if elapsed < 60 { return "Now" }
    let minutes = elapsed / 60
    if minutes < 60 { return "\(minutes)m" }
    let hours = minutes / 60
    if hours < 24 { return "\(hours)h" }
    return "\(hours / 24)d"
}

struct NotificationId: Codable, Equatable, Sendable {
    let coordinatorEpoch: String
    let sequence: UInt64

    enum CodingKeys: String, CodingKey {
        case coordinatorEpoch = "coordinator_epoch"
        case sequence
    }
}

struct RegistrationId: Codable, Equatable, Sendable {
    let coordinatorEpoch: String
    let sequence: UInt64

    enum CodingKeys: String, CodingKey {
        case coordinatorEpoch = "coordinator_epoch"
        case sequence
    }
}

struct NotificationTarget: Codable, Sendable {
    let workspaceId: String
    let tabId: String
    let terminalId: String

    enum CodingKeys: String, CodingKey {
        case workspaceId = "workspace_id"
        case tabId = "tab_id"
        case terminalId = "terminal_id"
    }
}

enum NotificationSource: String, Codable, Sendable, Equatable {
    case state, explicit
}

enum NotificationVisual: String, Codable, Sendable, Equatable {
    case none, gardn, terminal, system
}

enum NotificationSound: String, Codable, Sendable, Equatable {
    case none, done, request
}

struct StateNotification: Codable, Sendable {
    let id: NotificationId
    let source: NotificationSource
    let target: NotificationTarget?
    let title: String
    let body: String?
    let visual: NotificationVisual
    let sound: NotificationSound
    let createdAtUnixMs: UInt64
    let expiresAtUnixMs: UInt64

    enum CodingKeys: String, CodingKey {
        case id, source, target, title, body, visual, sound
        case createdAtUnixMs = "created_at_unix_ms"
        case expiresAtUnixMs = "expires_at_unix_ms"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(source, forKey: .source)
        try container.encode(target, forKey: .target)
        try container.encode(title, forKey: .title)
        try container.encode(body, forKey: .body)
        try container.encode(visual, forKey: .visual)
        try container.encode(sound, forKey: .sound)
        try container.encode(createdAtUnixMs, forKey: .createdAtUnixMs)
        try container.encode(expiresAtUnixMs, forKey: .expiresAtUnixMs)
    }
}

struct PresentationRequest: Codable, Sendable {
    let registrationId: RegistrationId
    let notification: StateNotification

    enum CodingKeys: String, CodingKey {
        case registrationId = "registration_id"
        case notification
    }
}

enum PresentationOutcome: Codable, Sendable {
    case submitted
    case rejected(String)
    case unknown

    private enum CodingKeys: String, CodingKey { case rejected }

    init(from decoder: Decoder) throws {
        let value = try decoder.singleValueContainer()
        if let name = try? value.decode(String.self) {
            switch name {
            case "submitted": self = .submitted
            case "unknown": self = .unknown
            default:
                throw DecodingError.dataCorruptedError(in: value, debugDescription: "Invalid presentation outcome")
            }
        } else {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            self = .rejected(try container.decode(String.self, forKey: .rejected))
        }
    }

    func encode(to encoder: Encoder) throws {
        switch self {
        case .submitted:
            var container = encoder.singleValueContainer()
            try container.encode("submitted")
        case .unknown:
            var container = encoder.singleValueContainer()
            try container.encode("unknown")
        case .rejected(let reason):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(reason, forKey: .rejected)
        }
    }
}

struct PresentationReceipt: Codable, Sendable {
    let registrationId: RegistrationId
    let notificationId: NotificationId
    let outcome: PresentationOutcome

    enum CodingKeys: String, CodingKey {
        case registrationId = "registration_id"
        case notificationId = "notification_id"
        case outcome
    }
}

struct PresenterRegistration: Codable {
    struct Capabilities: Codable {
        let terminal: Bool
        let system: Bool
        let sound: Bool
    }

    let name: String
    let renderingHostId: String
    let capabilities: Capabilities

    enum CodingKeys: String, CodingKey {
        case name, capabilities
        case renderingHostId = "rendering_host_id"
    }
}

struct PresenterRegistrationResponse: Codable {
    enum Kind: String, Codable {
        case registered = "notification_presenter_registered"
    }

    let type: Kind
    let registrationId: RegistrationId

    enum CodingKeys: String, CodingKey {
        case type
        case registrationId = "registration_id"
    }
}

struct PresenterCommand<Params: Codable>: Codable {
    let id: String
    let method: String
    let params: Params
}

enum PresenterEnvelope: Codable {
    case registered(id: String, PresenterRegistrationResponse)
    case presentation(PresentationRequest)

    private enum CodingKeys: String, CodingKey { case id, method, params, result }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        if container.contains(.method) {
            let method = try container.decode(String.self, forKey: .method)
            guard method == "notification.presentation" else {
                throw DecodingError.dataCorruptedError(forKey: .method, in: container, debugDescription: "Unexpected presenter method")
            }
            self = .presentation(try container.decode(PresentationRequest.self, forKey: .params))
        } else {
            self = .registered(
                id: try container.decode(String.self, forKey: .id),
                try container.decode(PresenterRegistrationResponse.self, forKey: .result)
            )
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .registered(let id, let response):
            try container.encode(id, forKey: .id)
            try container.encode(response, forKey: .result)
        case .presentation(let request):
            try container.encode("notification.presentation", forKey: .method)
            try container.encode(request, forKey: .params)
        }
    }
}

func connectUnixSocket(path: String) throws -> Int32 {
    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else { throw GardnClientError(message: "socket() failed") }
    var connected = false
    defer { if !connected { close(fd) } }
    var noSigPipe: Int32 = 1
    guard setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &noSigPipe, socklen_t(MemoryLayout<Int32>.size)) == 0 else {
        throw GardnClientError(message: "socket configuration failed")
    }
    var addr = sockaddr_un()
    addr.sun_family = sa_family_t(AF_UNIX)
    let maxPath = 104
    if path.utf8.count + 1 > maxPath {
        throw GardnClientError(message: "socket path too long")
    }
    withUnsafeMutablePointer(to: &addr) { addrPtr in
        let dest = UnsafeMutableRawPointer(addrPtr)
            .advanced(by: MemoryLayout<sockaddr_un>.offset(of: \.sun_path)!)
            .assumingMemoryBound(to: CChar.self)
        path.withCString { src in
            _ = strlcpy(dest, src, maxPath)
        }
    }

    let connectResult = withUnsafePointer(to: &addr) { ptr in
        ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockAddr in
            Darwin.connect(fd, sockAddr, socklen_t(MemoryLayout<sockaddr_un>.size))
        }
    }
    guard connectResult == 0 else {
        throw GardnClientError(message: "Gardn isn’t running")
    }
    connected = true
    return fd
}

private func unixRequest(path: String, payload: Data) throws -> Data {
    let fd = try connectUnixSocket(path: path)
    defer { close(fd) }

    try payload.withUnsafeBytes { buffer in
        var written = 0
        let bytes = buffer.bindMemory(to: UInt8.self)
        while written < bytes.count {
            let n = send(fd, bytes.baseAddress! + written, bytes.count - written, 0)
            if n <= 0 { throw GardnClientError(message: "write failed") }
            written += n
        }
    }

    var collected = Data()
    var chunk = [UInt8](repeating: 0, count: 4096)
    while true {
        let n = recv(fd, &chunk, chunk.count, 0)
        if n < 0 { throw GardnClientError(message: "read failed") }
        if n == 0 { break }
        collected.append(contentsOf: chunk.prefix(n))
        if collected.last == 0x0A { break }
    }
    if collected.last == 0x0A { collected.removeLast() }
    return collected
}

@MainActor
final class GardnPresenterStream {
    typealias RequestHandler = (PresentationRequest, @escaping @Sendable (PresentationOutcome) -> Void) -> Void

    enum State: Equatable {
        case idle
        case registering
        case registered(RegistrationId)
        case stopped
    }

    private(set) var state: State = .idle
    var active: Bool {
        switch state {
        case .registering, .registered: return true
        case .idle, .stopped: return false
        }
    }

    private static let registrationRequestId = "menu:notification.presenter.register"
    private let socketPath: String
    private let requestHandler: RequestHandler
    private var fd: Int32 = -1
    private var source: DispatchSourceRead?
    private var buffer = Data()

    init(socketPath: String, requestHandler: @escaping RequestHandler) {
        self.socketPath = socketPath
        self.requestHandler = requestHandler
    }

    func start() -> Bool {
        guard state == .idle else { return false }
        do {
            fd = try connectUnixSocket(path: socketPath)
            let registration = PresenterCommand(
                id: Self.registrationRequestId,
                method: "notification.presenter.register",
                params: PresenterRegistration(
                    name: "GardnMenu",
                    renderingHostId: Host.current().localizedName ?? "macos",
                    capabilities: .init(terminal: false, system: true, sound: true)
                )
            )
            var line = try JSONEncoder().encode(registration)
            line.append(0x0A)
            try line.withUnsafeBytes { bytes in
                var offset = 0
                while offset < bytes.count {
                    let count = Darwin.send(fd, bytes.baseAddress!.advanced(by: offset), bytes.count - offset, 0)
                    guard count > 0 else { throw GardnClientError(message: "write failed") }
                    offset += count
                }
            }
        } catch {
            if fd >= 0 { close(fd) }
            fd = -1
            state = .stopped
            return false
        }
        let source = DispatchSource.makeReadSource(fileDescriptor: fd, queue: .main)
        source.setEventHandler { [weak self] in
            MainActor.assumeIsolated { self?.readAvailable() }
        }
        source.setCancelHandler { [fd] in close(fd) }
        self.source = source
        state = .registering
        source.activate()
        return true
    }

    func cancel() {
        guard active else { return }
        state = .stopped
        shutdown(fd, SHUT_RDWR)
        source?.cancel()
        source = nil
        fd = -1
        buffer.removeAll()
    }

    deinit {
        source?.cancel()
    }

    private func readAvailable() {
        guard active else { return }
        var chunk = [UInt8](repeating: 0, count: 4096)
        let count = recv(fd, &chunk, chunk.count, 0)
        guard count > 0 else {
            cancel()
            return
        }
        buffer.append(contentsOf: chunk.prefix(count))
        while active, let newline = buffer.firstIndex(of: 0x0A) {
            let line = Data(buffer.prefix(upTo: newline))
            buffer.removeSubrange(...newline)
            guard let envelope = try? JSONDecoder().decode(PresenterEnvelope.self, from: line) else {
                cancel()
                return
            }
            switch envelope {
            case .registered(let id, let response):
                guard state == .registering, id == Self.registrationRequestId else {
                    cancel()
                    return
                }
                state = .registered(response.registrationId)
            case .presentation(let request):
                guard case .registered(let registrationId) = state,
                      request.registrationId == registrationId else {
                    cancel()
                    return
                }
                requestHandler(request) { [weak self] outcome in
                    Task { @MainActor [weak self] in
                        self?.sendReceipt(PresentationReceipt(
                            registrationId: request.registrationId,
                            notificationId: request.notification.id,
                            outcome: outcome
                        ))
                    }
                }
            }
        }
    }

    private func sendReceipt(_ receipt: PresentationReceipt) {
        guard case .registered(let registrationId) = state,
              receipt.registrationId == registrationId else { return }
        let request = PresenterCommand(
            id: "menu:notification.presenter.receipt",
            method: "notification.presenter.receipt",
            params: receipt
        )
        let socketPath = socketPath
        Task.detached(priority: .utility) {
            guard var line = try? JSONEncoder().encode(request) else { return }
            line.append(0x0A)
            _ = try? unixRequest(path: socketPath, payload: line)
        }
    }
}
