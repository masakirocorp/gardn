import Foundation
import os

enum BundledGardn {
    private static let log = Logger(subsystem: "com.masakiro.gardn.menu", category: "bundled-gardn")

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

    static func logFailure(_ error: Error) {
        log.error("bundled gardn failed: \(error.localizedDescription, privacy: .public)")
    }
}
