import Foundation

enum BundledGardn {
    private static let fm = FileManager.default
    private static let home = fm.homeDirectoryForCurrentUser

    static let cliName = "gardn"

    static var binaryURL: URL? {
        guard let folder = Bundle.main.executableURL?.deletingLastPathComponent() else {
            return nil
        }
        let url = folder.appendingPathComponent("gardn")
        return fm.isExecutableFile(atPath: url.path) ? url : nil
    }

    static var symlinkURL: URL {
        home.appendingPathComponent(".local/bin/\(cliName)")
    }

    static var installURL: URL {
        home.appendingPathComponent(".local/share/gardn/cli/\(cliName)")
    }

    static var installStatus: InstallStatus {
        let symlink = symlinkURL.path
        if let dest = try? fm.destinationOfSymbolicLink(atPath: symlink) {
            let resolved = dest.hasPrefix("/")
                ? URL(fileURLWithPath: dest)
                : symlinkURL.deletingLastPathComponent().appendingPathComponent(dest)
            if resolved.standardizedFileURL == installURL.standardizedFileURL {
                return .installed
            }
            return .external(resolved.path)
        }
        if fm.fileExists(atPath: symlink) {
            return .external(symlink)
        }
        return .missing
    }

    static var pathHint: String? {
        let path = ProcessInfo.processInfo.environment["PATH"] ?? ""
        let localBin = home.appendingPathComponent(".local/bin").path
        if path.split(separator: ":").contains(where: { String($0) == localBin }) {
            return nil
        }
        return "Add ~/.local/bin to PATH to use \(cliName) in terminals."
    }

    static func process(arguments: [String]) throws -> Process {
        guard let binaryURL else {
            throw GardnClientError(message: "This extra has no bundled gardn")
        }
        let process = Process()
        process.executableURL = binaryURL
        process.arguments = arguments
        return process
    }

    static func refreshInstalledCLIIfOwned() {
        guard case .installed = installStatus else { return }
        try? installCLI()
    }

    static func installCLI() throws {
        guard let binaryURL else {
            throw GardnClientError(message: "This extra has no bundled gardn")
        }
        let binDir = symlinkURL.deletingLastPathComponent()
        let installDir = installURL.deletingLastPathComponent()
        try fm.createDirectory(at: binDir, withIntermediateDirectories: true)
        try fm.createDirectory(at: installDir, withIntermediateDirectories: true)
        if fm.fileExists(atPath: installURL.path) {
            try fm.removeItem(at: installURL)
        }
        try fm.copyItem(at: binaryURL, to: installURL)
        try fm.setAttributes([.posixPermissions: 0o755], ofItemAtPath: installURL.path)
        if fm.fileExists(atPath: symlinkURL.path)
            || (try? fm.destinationOfSymbolicLink(atPath: symlinkURL.path)) != nil
        {
            try fm.removeItem(at: symlinkURL)
        }
        try fm.createSymbolicLink(at: symlinkURL, withDestinationURL: installURL)
    }

    static func uninstallCLI() throws {
        guard case .installed = installStatus else { return }
        try? fm.removeItem(at: symlinkURL)
        try? fm.removeItem(at: installURL)
    }

    enum InstallStatus: Equatable {
        case missing
        case installed
        case external(String)

        var label: String {
            switch self {
            case .missing:
                return "Not installed"
            case .installed:
                return "Installed at ~/.local/bin/\(BundledGardn.cliName)"
            case let .external(path):
                return "A different \(BundledGardn.cliName) is at \(path)"
            }
        }
    }
}
