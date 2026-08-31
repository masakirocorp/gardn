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

    enum Section: String, CaseIterable {
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
    var focused: Bool

    var section: Section {
        if followUp { return .followUp }
        switch status {
        case .blocked, .done: return .triage
        case .working: return .working
        case .idle, .unknown: return .idle
        }
    }

    var needsAttention: Bool {
        followUp || status == .blocked || status == .done
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
        var agentsInWorkspace: [String: Int] = [:]
        var agentsInTab: [String: Int] = [:]
        for raw in agents {
            if let workspaceId = raw["workspace_id"] as? String {
                agentsInWorkspace[workspaceId, default: 0] += 1
            }
            if let tabId = raw["tab_id"] as? String {
                agentsInTab[tabId, default: 0] += 1
            }
        }

        return agents.compactMap { raw in
            guard let terminalId = raw["terminal_id"] as? String else { return nil }
            let status = AgentRecord.Status(rawValue: raw["agent_status"] as? String ?? "unknown") ?? .unknown
            if status == .unknown, raw["agent"] == nil, raw["display_agent"] == nil, raw["name"] == nil {
                return nil
            }
            let workspaceId = raw["workspace_id"] as? String
            let workspace = workspaceId.flatMap { workspaceById[$0] }
            let tabId = raw["tab_id"] as? String
            let tab = tabId.flatMap { tabById[$0] }
            let groupId = workspace?["group_id"] as? String
            let group = groupId.flatMap { groupById[$0] }

            var title = (workspace?["label"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
            if title == nil || title?.isEmpty == true {
                let cwd = (raw["foreground_cwd"] as? String) ?? (raw["cwd"] as? String)
                title = cwd.flatMap { URL(fileURLWithPath: $0).lastPathComponent }
                    ?? (raw["name"] as? String)
                    ?? terminalId
            }
            if let workspaceId, agentsInWorkspace[workspaceId, default: 0] > 1,
               let tabLabel = tab?["label"] as? String, isUsefulTabLabel(tabLabel)
            {
                title = "\(title!) / \(tabLabel)"
            }
            if let tabId, agentsInTab[tabId, default: 0] > 1,
               let paneLabel = raw["name"] as? String, !paneLabel.isEmpty
            {
                title = "\(title!) / \(paneLabel)"
            }

            let followUp = boolValue(raw["follow_up"])
            let focused = boolValue(raw["focused"])
            let age = activityAge(
                unixSecs: unixSecs(raw["follow_up_added_at_unix_secs"])
                    ?? unixSecs(raw["last_meaningful_agent_activity_unix_secs"])
            )
            return AgentRecord(
                terminalId: terminalId,
                title: title ?? terminalId,
                groupName: group?["name"] as? String,
                groupAccent: group?["accent"] as? String,
                status: status,
                statusLabel: statusText(status),
                age: age,
                followUp: followUp,
                focused: focused
            )
        }
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

private func unixRequest(path: String, payload: Data) throws -> Data {
    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else { throw GardnClientError(message: "socket() failed") }
    defer { close(fd) }

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
