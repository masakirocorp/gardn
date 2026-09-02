import Darwin
import Foundation
import os

enum PathCli {
    private static let log = Logger(subsystem: "com.masakiro.gardn.menu", category: "path-cli")
    private static let fileManager = FileManager.default

    static func installBundledCLI() {
        #if DEBUG
            return
        #else
            do {
                guard shouldClaimPath(bundleURL: Bundle.main.bundleURL) else {
                    return
                }
                let bundledCLI = try BundledGardn.binaryURL()
                let binDirectory = fileManager.homeDirectoryForCurrentUser
                    .appendingPathComponent(".local", isDirectory: true)
                    .appendingPathComponent("bin", isDirectory: true)
                try installShim(from: bundledCLI, into: binDirectory)
            } catch {
                log.error(
                    "Unable to install PATH shim: \(error.localizedDescription, privacy: .public)"
                )
            }
        #endif
    }

    static func shouldClaimPath(bundleURL: URL) -> Bool {
        let appURL = bundleURL.standardizedFileURL.resolvingSymlinksInPath()
        let path = appURL.path
        if path.contains("/AppTranslocation/") {
            return false
        }
        if path.hasPrefix("/Volumes/") {
            return false
        }
        guard appURL.lastPathComponent == "Gardn.app" else {
            return false
        }
        return appURL.deletingLastPathComponent().lastPathComponent == "Applications"
    }

    static func installShim(from bundledCLI: URL, into binDirectory: URL) throws {
        guard fileManager.isExecutableFile(atPath: bundledCLI.path) else {
            throw GardnClientError(message: "This app is missing its bundled CLI")
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
        } else {
            var isDirectory = ObjCBool(false)
            if fileManager.fileExists(atPath: shim.path, isDirectory: &isDirectory),
                isDirectory.boolValue
            {
                throw GardnClientError(
                    message: "Unable to install PATH shim: \(shim.path) is a directory"
                )
            }
        }

        let temporary = binDirectory.appendingPathComponent(".gardn-\(UUID().uuidString)")
        try fileManager.createSymbolicLink(at: temporary, withDestinationURL: bundledCLI)
        let status = rename(temporary.path, shim.path)
        if status != 0 {
            try? fileManager.removeItem(at: temporary)
            throw GardnClientError(
                message:
                    "Unable to install PATH shim: \(String(cString: strerror(errno)))"
            )
        }
    }
}
