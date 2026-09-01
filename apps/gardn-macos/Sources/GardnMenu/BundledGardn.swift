import Foundation

enum BundledGardn {
    static var binaryURL: URL? {
        guard let folder = Bundle.main.executableURL?.deletingLastPathComponent() else {
            return nil
        }
        let url = folder.appendingPathComponent("gardn")
        return FileManager.default.isExecutableFile(atPath: url.path) ? url : nil
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
}
