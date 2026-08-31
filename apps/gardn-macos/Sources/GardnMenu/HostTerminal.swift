import AppKit
import Darwin
import Foundation

enum HostTerminal {
    static func raise(clientSocketPath: String, titleHint _: String? = nil) {
        let apiSocketPath = clientSocketPath.replacingOccurrences(of: "-client.sock", with: ".sock")
        var seen = Set<pid_t>()
        for pid in pidsHolding(unixSocket: clientSocketPath) + pidsHolding(unixSocket: apiSocketPath) {
            guard let app = hostingApp(from: pid) else { continue }
            guard seen.insert(app.processIdentifier).inserted else { continue }
            activate(app)
        }
    }

    private static func activate(_ app: NSRunningApplication) {
        NSApp.activate()
        _ = app.activate(from: NSRunningApplication.current)
    }

    private static func pidsHolding(unixSocket path: String) -> [pid_t] {
        guard FileManager.default.fileExists(atPath: path) else { return [] }
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/usr/sbin/lsof")
        proc.arguments = ["-t", "--", path]
        let stdout = Pipe()
        proc.standardOutput = stdout
        proc.standardError = FileHandle.nullDevice
        do {
            try proc.run()
            proc.waitUntilExit()
        } catch {
            return []
        }
        let text = String(data: stdout.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let selfPid = ProcessInfo.processInfo.processIdentifier
        return text.split { $0.isNewline }.compactMap { pid_t($0) }.filter { $0 != selfPid }
    }

    private static func hostingApp(from pid: pid_t) -> NSRunningApplication? {
        var current = pid
        var seen = Set<pid_t>()
        let extraId = Bundle.main.bundleIdentifier
        for _ in 0..<24 {
            if !seen.insert(current).inserted || current <= 1 {
                break
            }
            if let app = NSRunningApplication(processIdentifier: current),
               app.activationPolicy == .regular,
               app.bundleIdentifier != extraId
            {
                return app
            }
            guard let parent = parentPID(current), parent != current else { break }
            current = parent
        }
        return nil
    }

    private static func parentPID(_ pid: pid_t) -> pid_t? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.stride)
        let n = proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size)
        guard n == size else { return nil }
        return pid_t(info.pbi_ppid)
    }
}
