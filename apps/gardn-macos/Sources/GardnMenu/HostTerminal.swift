import AppKit
import Darwin
import Foundation

enum HostTerminal {
    static func raise(apiSocketPath: String, coordinator: ExtraCoordinator? = nil) {
        Task.detached(priority: .userInitiated) {
            let pids = clientPids(matchingApiSocket: apiSocketPath)
            await MainActor.run {
                var seen = Set<pid_t>()
                for pid in pids {
                    guard let app = hostingApp(from: pid) else { continue }
                    guard seen.insert(app.processIdentifier).inserted else { continue }
                    activate(app)
                }
                if seen.isEmpty {
                    launchClient(coordinator)
                }
            }
        }
    }

    private static func launchClient(_ coordinator: ExtraCoordinator?) {
        var arguments: [String] = []
        if coordinator?.kind == .remote, let target = coordinator?.target {
            arguments.append(contentsOf: ["--remote", target])
            if let session = coordinator?.session, session != "default" {
                arguments.append(contentsOf: ["--session", session])
            }
        } else if let session = coordinator?.session, session != "default" {
            arguments.append(contentsOf: ["--session", session])
        }
        do {
            let process = try BundledGardn.process(arguments: arguments)
            try process.run()
        } catch {
            BundledGardn.logFailure(error)
        }
    }

    private static func activate(_ app: NSRunningApplication) {
        NSApp.activate()
        _ = app.activate(from: NSRunningApplication.current)
    }

    private static func clientPids(matchingApiSocket apiSocketPath: String) -> [pid_t] {
        var capacity = proc_listallpids(nil, 0)
        guard capacity > 0 else { return [] }
        var pids = [pid_t](repeating: 0, count: Int(capacity))
        capacity = proc_listallpids(&pids, Int32(MemoryLayout<pid_t>.stride * pids.count))
        guard capacity > 0 else { return [] }
        let namedSession = apiSocketPath.contains("/sessions/")
        return pids.prefix(Int(capacity)).filter { pid in
            var name = [CChar](repeating: 0, count: Int(MAXPATHLEN))
            guard proc_name(pid, &name, UInt32(name.count)) > 0 else { return false }
            let process = String(cString: name)
            guard process == "gardn" || process == "gardn-dev" else { return false }
            let info = procArgs(pid)
            if info.args.contains("server") {
                return false
            }
            if !namedSession, info.args.contains("--session") {
                return false
            }
            if let socket = info.env["GARDN_SOCKET_PATH"], !socket.isEmpty {
                return socket == apiSocketPath
            }
            return true
        }
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

    private struct ProcArgs {
        var args: [String] = []
        var env: [String: String] = [:]
    }

    private static func procArgs(_ pid: pid_t) -> ProcArgs {
        var mib: [Int32] = [CTL_KERN, KERN_PROCARGS2, pid]
        var size = 0
        guard sysctl(&mib, 3, nil, &size, nil, 0) == 0, size > 4 else { return ProcArgs() }
        var buffer = [UInt8](repeating: 0, count: size)
        guard sysctl(&mib, 3, &buffer, &size, nil, 0) == 0, size > 4 else { return ProcArgs() }
        return parseProcArgs2(buffer)
    }

    private static func parseProcArgs2(_ buffer: [UInt8]) -> ProcArgs {
        var result = ProcArgs()
        guard buffer.count > 4 else { return result }
        let argc = Int(buffer[0]) | Int(buffer[1]) << 8 | Int(buffer[2]) << 16 | Int(buffer[3]) << 24
        guard argc >= 0, argc < 256 else { return result }
        var i = 4
        while i < buffer.count, buffer[i] != 0 {
            i += 1
        }
        i += 1
        while i < buffer.count, buffer[i] == 0 {
            i += 1
        }
        func nextString() -> String? {
            guard i < buffer.count else { return nil }
            if buffer[i] == 0 {
                return nil
            }
            let start = i
            while i < buffer.count, buffer[i] != 0 {
                i += 1
            }
            let bytes = buffer[start..<i]
            i += 1
            return String(bytes: bytes, encoding: .utf8)
        }
        for _ in 0..<argc {
            guard let arg = nextString() else { break }
            result.args.append(arg)
        }
        while let entry = nextString() {
            if let eq = entry.firstIndex(of: "=") {
                result.env[String(entry[..<eq])] = String(entry[entry.index(after: eq)...])
            }
        }
        return result
    }
}
