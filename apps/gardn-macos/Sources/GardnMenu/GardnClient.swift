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
    var subtitle: String
    var status: Status
    var followUp: Bool
    var cwd: String?

    var section: Section {
        if followUp {
            return .followUp
        }
        switch status {
        case .blocked, .done:
            return .triage
        case .working:
            return .working
        case .idle, .unknown:
            return .idle
        }
    }

    var needsAttention: Bool {
        followUp || status == .blocked || status == .done
    }
}

struct GardnClient {
    var socketPath: String

    static func defaultSocketPath() -> String {
        if let override = ProcessInfo.processInfo.environment["GARDN_SOCKET_PATH"], !override.isEmpty {
            return override
        }
        let home = FileManager.default.homeDirectoryForCurrentUser
        let primary = home.appendingPathComponent(".config/gardn/gardn.sock").path
        if FileManager.default.fileExists(atPath: primary) {
            return primary
        }
        return home.appendingPathComponent(".config/gardn-dev/gardn.sock").path
    }

    func listAgents() throws -> [AgentRecord] {
        let json = try transact([
            "id": "menu:agent.list",
            "method": "agent.list",
            "params": [:],
        ])
        let result = try resultObject(json)
        let type = result["type"] as? String
        guard type == "agent_list" else {
            throw GardnClientError(message: "unexpected result type \(type ?? "nil")")
        }
        let raw = result["agents"] as? [[String: Any]] ?? []
        var agents = raw.compactMap(Self.parseAgent)
        Self.disambiguateTitles(&agents)
        return agents
    }

    func focus(terminalId: String) throws {
        _ = try transact([
            "id": "menu:agent.focus",
            "method": "agent.focus",
            "params": ["target": terminalId],
        ])
    }

    private func resultObject(_ json: [String: Any]) throws -> [String: Any] {
        if let error = json["error"] as? [String: Any] {
            let message = error["message"] as? String ?? "request failed"
            throw GardnClientError(message: message)
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

    private static func parseAgent(_ raw: [String: Any]) -> AgentRecord? {
        guard let terminalId = raw["terminal_id"] as? String else {
            return nil
        }
        let status = AgentRecord.Status(rawValue: raw["agent_status"] as? String ?? "unknown") ?? .unknown
        if status == .unknown, raw["agent"] == nil, raw["display_agent"] == nil, raw["name"] == nil {
            return nil
        }
        let cwd = (raw["foreground_cwd"] as? String) ?? (raw["cwd"] as? String)
        let leaf = cwd.flatMap { URL(fileURLWithPath: $0).lastPathComponent }
        let title = leaf
            ?? (raw["name"] as? String)
            ?? (raw["display_agent"] as? String)
            ?? (raw["agent"] as? String)
            ?? terminalId
        let agent = (raw["display_agent"] as? String) ?? (raw["agent"] as? String)
        let custom = raw["custom_status"] as? String
        let followUp = (raw["follow_up"] as? Bool) ?? (raw["follow_up"] as? NSNumber)?.boolValue ?? false
        let age = Self.activityAge(
            unixSecs: Self.unixSecs(raw["follow_up_added_at_unix_secs"])
                ?? Self.unixSecs(raw["last_meaningful_agent_activity_unix_secs"])
        )
        let subtitle = [agent, custom, age].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · ")
        return AgentRecord(
            terminalId: terminalId,
            title: title,
            subtitle: subtitle,
            status: status,
            followUp: followUp,
            cwd: cwd
        )
    }

    private static func disambiguateTitles(_ agents: inout [AgentRecord]) {
        var counts: [String: Int] = [:]
        for agent in agents {
            counts[agent.title, default: 0] += 1
        }
        for index in agents.indices {
            guard counts[agents[index].title, default: 0] > 1 else { continue }
            guard let cwd = agents[index].cwd else { continue }
            let parent = URL(fileURLWithPath: cwd).deletingLastPathComponent().lastPathComponent
            guard !parent.isEmpty, parent != "/" else { continue }
            agents[index].title = "\(parent)/\(agents[index].title)"
        }
    }

    private static func unixSecs(_ value: Any?) -> UInt64? {
        if let n = value as? NSNumber {
            return n.uint64Value
        }
        if let n = value as? UInt64 {
            return n
        }
        if let n = value as? Int, n >= 0 {
            return UInt64(n)
        }
        return nil
    }

    private static func activityAge(unixSecs: UInt64?) -> String? {
        guard let unixSecs else { return nil }
        let now = UInt64(Date().timeIntervalSince1970)
        let elapsed = now > unixSecs ? now - unixSecs : 0
        if elapsed < 60 {
            return "Now"
        }
        let minutes = elapsed / 60
        if minutes < 60 {
            return "\(minutes)m"
        }
        let hours = minutes / 60
        if hours < 24 {
            return "\(hours)h"
        }
        return "\(hours / 24)d"
    }
}

private func unixRequest(path: String, payload: Data) throws -> Data {
    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else {
        throw GardnClientError(message: "socket() failed")
    }
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
            if n <= 0 {
                throw GardnClientError(message: "write failed")
            }
            written += n
        }
    }

    var collected = Data()
    var chunk = [UInt8](repeating: 0, count: 4096)
    while true {
        let n = recv(fd, &chunk, chunk.count, 0)
        if n < 0 {
            throw GardnClientError(message: "read failed")
        }
        if n == 0 {
            break
        }
        collected.append(contentsOf: chunk.prefix(n))
        if collected.last == 0x0A {
            break
        }
    }
    if collected.last == 0x0A {
        collected.removeLast()
    }
    return collected
}
