import Foundation
import os

enum PathCli {
    private static let log = Logger(subsystem: "com.masakiro.gardn.menu", category: "path-cli")
    private static let fileManager = FileManager.default

    static func installBundledCLI() {
        do {
            guard let executableURL = Bundle.main.executableURL else {
                log.error("Unable to install PATH shim: app executable URL is unavailable")
                return
            }
            let bundledCLI = executableURL
                .deletingLastPathComponent()
                .appendingPathComponent("gardn")
            let binDirectory = fileManager.homeDirectoryForCurrentUser
                .appendingPathComponent(".local", isDirectory: true)
                .appendingPathComponent("bin", isDirectory: true)
            try installShim(from: bundledCLI, into: binDirectory)
        } catch {
            log.error("Unable to install PATH shim: \(error.localizedDescription, privacy: .public)")
        }
    }

    static func installShim(from bundledCLI: URL, into binDirectory: URL) throws {
        guard fileManager.isExecutableFile(atPath: bundledCLI.path) else {
            log.error("Unable to install PATH shim: bundled gardn is unavailable")
            return
        }

        try fileManager.createDirectory(at: binDirectory, withIntermediateDirectories: true)

        let shim = binDirectory.appendingPathComponent("gardn")
        if let destination = try? fileManager.destinationOfSymbolicLink(atPath: shim.path) {
            let destinationURL = URL(fileURLWithPath: destination, relativeTo: binDirectory)
                .standardizedFileURL
                .resolvingSymlinksInPath()
            let bundledURL = bundledCLI.standardizedFileURL.resolvingSymlinksInPath()
            if destinationURL.path == bundledURL.path {
                return
            }
            try fileManager.removeItem(at: shim)
        } else {
            var isDirectory = ObjCBool(false)
            if fileManager.fileExists(atPath: shim.path, isDirectory: &isDirectory) {
                if isDirectory.boolValue {
                    log.error(
                        "Unable to install PATH shim: \(shim.path, privacy: .public) is a directory"
                    )
                    return
                }
                try fileManager.removeItem(at: shim)
            }
        }

        try fileManager.createSymbolicLink(at: shim, withDestinationURL: bundledCLI)
    }
}
