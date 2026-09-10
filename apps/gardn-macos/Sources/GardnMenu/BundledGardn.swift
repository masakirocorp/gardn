import Foundation
import os

struct RuntimeStatus: Codable, Equatable, Sendable {
    enum State: String, Codable, Sendable {
        case current
        case serverRestartRequired = "server_restart_required"
        case clientUpdateRequired = "client_update_required"
        case versionSkew = "version_skew"
        case protocolIncompatible = "protocol_incompatible"
        case unknown
        case serverNotRunning = "server_not_running"
    }

    enum Action: String, Codable, Sendable {
        case none
        case restartServer = "restart_server"
        case updateClient = "update_client"
        case inspectStatus = "inspect_status"
    }

    let state: State
    let action: Action
    let clientVersion: String
    let clientProtocol: UInt32
    let serverVersion: String?
    let serverProtocol: UInt32?
    let liveHandoff: Bool?

    enum CodingKeys: String, CodingKey {
        case state, action
        case clientVersion = "client_version"
        case clientProtocol = "client_protocol"
        case serverVersion = "server_version"
        case serverProtocol = "server_protocol"
        case liveHandoff = "live_handoff"
    }

    static func decode(_ data: Data) throws -> RuntimeStatus {
        struct StatusOutput: Decodable {
            let runtime: RuntimeStatus
        }
        return try JSONDecoder().decode(StatusOutput.self, from: data).runtime
    }
}

enum GardnInstallation: Equatable, Sendable {
    case matching
    case mismatch(app: String, cli: String)
    case unknown

    static func compare(appVersion: String?, cliVersion: String?) -> Self {
        guard let appVersion, !appVersion.isEmpty,
              let cliVersion, !cliVersion.isEmpty else { return .unknown }
        return appVersion == cliVersion ? .matching : .mismatch(app: appVersion, cli: cliVersion)
    }
}

struct RuntimeNotice: Equatable, Sendable {
    let title: String
    let detail: String

    static let unknown = RuntimeNotice(
        title: "Unable to verify Gardn versions",
        detail: "Run gardn status --json for the selected coordinator to inspect its status."
    )

    static func presentation(runtime: RuntimeStatus?, installation: GardnInstallation) -> Self? {
        if case let .mismatch(app, cli) = installation {
            return Self(
                title: "Gardn installation needs repair",
                detail: "The app is v\(app), but its bundled CLI is v\(cli). Reinstall Gardn."
            )
        }
        guard let runtime else { return .unknown }
        switch runtime.state {
        case .current, .serverNotRunning:
            return nil
        case .serverRestartRequired:
            guard let server = runtime.serverVersion else { return .unknown }
            return Self(
                title: "Server v\(server) is still running",
                detail: "Restart it to use v\(runtime.clientVersion)."
            )
        case .clientUpdateRequired:
            guard let server = runtime.serverVersion else { return .unknown }
            return Self(
                title: "Gardn v\(runtime.clientVersion) is out of date",
                detail: "Update Gardn to match server v\(server)."
            )
        case .protocolIncompatible:
            guard let server = runtime.serverProtocol else { return .unknown }
            return Self(
                title: "Gardn versions cannot connect",
                detail: "Client protocol \(runtime.clientProtocol) does not match server protocol \(server)."
            )
        case .versionSkew:
            guard let server = runtime.serverVersion else { return .unknown }
            return Self(
                title: "Gardn versions differ",
                detail: "Gardn v\(runtime.clientVersion) can connect to server v\(server). Run gardn status --json to inspect."
            )
        case .unknown:
            return .unknown
        }
    }
}

enum BundledGardn {
    private static let log = Logger(subsystem: "com.masakiro.gardn.menu", category: "bundled-gardn")

    enum SoundPlaybackOutcome: Sendable {
        case played
        case suppressed
        case failed(String)
    }

    static func binaryURL() throws -> URL {
        guard let folder = Bundle.main.executableURL?.deletingLastPathComponent() else {
            throw GardnClientError(message: "This app is missing its bundled CLI")
        }
        let url = folder.appendingPathComponent("gardn-cli")
        guard FileManager.default.isExecutableFile(atPath: url.path) else {
            throw GardnClientError(message: "This app is missing its bundled CLI")
        }
        return url
    }

    static func process(arguments: [String]) throws -> Process {
        let process = Process()
        process.executableURL = try binaryURL()
        process.arguments = arguments
        return process
    }

    static func runtimeStatus(socketPath: String) async throws -> RuntimeStatus {
        try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .utility).async {
                do {
                    let process = try process(arguments: ["status", "--json"])
                    var environment = ProcessInfo.processInfo.environment
                    environment["GARDN_SOCKET_PATH"] = socketPath
                    process.environment = environment
                    let stdout = Pipe()
                    process.standardOutput = stdout
                    process.standardError = FileHandle.nullDevice
                    process.standardInput = FileHandle.nullDevice
                    try process.run()
                    let timeout = DispatchWorkItem {
                        if process.isRunning { process.terminate() }
                    }
                    DispatchQueue.global(qos: .utility).asyncAfter(
                        deadline: .now() + .seconds(10), execute: timeout
                    )
                    defer { timeout.cancel() }
                    let data = stdout.fileHandleForReading.readDataToEndOfFile()
                    process.waitUntilExit()
                    guard process.terminationReason == .exit, process.terminationStatus == 0 else {
                        throw GardnClientError(message: "Unable to inspect Gardn runtime status")
                    }
                    continuation.resume(returning: try RuntimeStatus.decode(data))
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    static func playSound(_ sound: NotificationSound) async -> SoundPlaybackOutcome {
        guard sound != .none else { return .suppressed }

        return await withCheckedContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                do {
                    let process = try process(arguments: ["sound", "play", sound.rawValue])
                    process.standardOutput = FileHandle.nullDevice
                    process.standardError = FileHandle.nullDevice
                    process.standardInput = FileHandle.nullDevice
                    try process.run()
                    process.waitUntilExit()
                    guard process.terminationReason == .exit else {
                        continuation.resume(returning: .failed("sound helper terminated"))
                        return
                    }
                    switch process.terminationStatus {
                    case 0:
                        continuation.resume(returning: .played)
                    case 3:
                        continuation.resume(returning: .suppressed)
                    default:
                        continuation.resume(
                            returning: .failed(
                                "sound helper exited with status \(process.terminationStatus)"
                            )
                        )
                    }
                } catch {
                    log.error("bundled sound failed: \(error.localizedDescription, privacy: .public)")
                    continuation.resume(returning: .failed(error.localizedDescription))
                }
            }
        }
    }

    static func logFailure(_ error: Error) {
        log.error("bundled gardn failed: \(error.localizedDescription, privacy: .public)")
    }
}
