import AppKit
import Sparkle
import SwiftUI
import UserNotifications

@main
struct GardnMenuApp: App {
    @NSApplicationDelegateAdaptor(ExtraAppDelegate.self) private var delegate

    var body: some Scene {
        Settings {
            ExtraSettingsView(
                store: delegate.store,
                catalog: delegate.store.catalog,
                checkForUpdates: ExtraAppDelegate.checkForUpdates
            )
        }
        .defaultSize(width: 560, height: 420)
        .commands {
            CommandGroup(after: .appInfo) {
                Button("Check for Updates…", action: ExtraAppDelegate.checkForUpdates)
            }
        }
    }
}

@MainActor
final class ExtraAppDelegate: NSObject, NSApplicationDelegate {
    static let updaterController = SPUStandardUpdaterController(
        startingUpdater: true,
        updaterDelegate: nil,
        userDriverDelegate: nil
    )

    static func checkForUpdates() {
        updaterController.updater.checkForUpdates()
    }
    let store = AgentStore()
    private let statusItem = NSStatusBar.system.statusItem(withLength: 22)
    private let popover = NSPopover()

    func applicationDidFinishLaunching(_ notification: Notification) {
        Self.terminateOtherCopies()
        PathCli.installBundledCLI()
        NSApp.setActivationPolicy(.accessory)
        UNUserNotificationCenter.current().delegate = self
        AgentNotifications.requestAuthorization()
        popover.behavior = .transient
        popover.animates = false
        popover.contentViewController = NSHostingController(
            rootView: AgentPanelView(store: store, catalog: store.catalog)
        )
        popover.contentSize = NSSize(width: 268, height: 420)
        statusItem.button?.imagePosition = .imageOnly
        statusItem.button?.action = #selector(togglePopover)
        statusItem.button?.target = self
        store.onNeedsAttentionChange = { [weak self] alert in
            self?.applyIcon(alert)
        }
        store.onDidFocus = { [weak self] in
            self?.popover.performClose(nil)
        }
        store.onOpenSettings = { [weak self] in
            self?.openSettings()
        }
        store.start()
        applyIcon(store.needsAttention)
    }

    private static func terminateOtherCopies() {
        let id = Bundle.main.bundleIdentifier ?? "com.masakiro.gardn.menu"
        let me = NSRunningApplication.current
        for app in NSRunningApplication.runningApplications(withBundleIdentifier: id) where app != me {
            app.forceTerminate()
        }
    }


    func applyIcon(_ alert: Bool) {
        statusItem.button?.image = StatusItemImage.make(alert: alert)
    }
    @objc private func togglePopover(_ sender: Any?) {
        guard let button = statusItem.button else { return }
        if popover.isShown {
            popover.performClose(sender)
        } else {
            NSApp.activate(ignoringOtherApps: true)
            store.refresh()
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        }
    }

    func openSettings() {
        popover.performClose(nil)
        NSApp.activate(ignoringOtherApps: true)
        NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil)
    }
}

extension ExtraAppDelegate: UNUserNotificationCenterDelegate {
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .list, .sound])
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        let terminalId = response.notification.request.content.userInfo[AgentNotifications.terminalIdKey] as? String
        Task { @MainActor in
            if let terminalId {
                store.focus(terminalId: terminalId)
            }
            completionHandler()
        }
    }
}


enum StatusItemImage {
    static func make(alert: Bool) -> NSImage {
        let size = NSSize(width: 22, height: 22)
        let image = NSImage(size: size, flipped: false) { rect in
            let inset = rect.insetBy(dx: 2.5, dy: 1)
            if alert {
                NSColor.black.withAlphaComponent(0.3).setFill()

                leafFaces(in: inset).fill()
            }
            NSColor.black.setStroke()
            let stroke = leafStroke(in: inset)
            stroke.lineWidth = 1.4
            stroke.lineJoinStyle = .round
            stroke.lineCapStyle = .round
            stroke.stroke()
            return true
        }
        image.isTemplate = true
        return image
    }

    /// Logo leaf, viewBox 70 28 116 164, no land plot.
    private static func map(_ x: CGFloat, _ y: CGFloat, in rect: NSRect) -> NSPoint {
        NSPoint(
            x: rect.minX + (x - 70) / 116 * rect.width,
            y: rect.maxY - (y - 28) / 164 * rect.height
        )
    }

    private static func leafFaces(in rect: NSRect) -> NSBezierPath {
        let path = NSBezierPath()
        path.move(to: map(128, 38, in: rect))
        path.line(to: map(176, 72, in: rect))
        path.line(to: map(128, 112, in: rect))
        path.line(to: map(80, 72, in: rect))
        path.close()
        path.move(to: map(80, 72, in: rect))
        path.line(to: map(80, 140, in: rect))
        path.line(to: map(128, 180, in: rect))
        path.line(to: map(128, 112, in: rect))
        path.close()
        path.move(to: map(176, 72, in: rect))
        path.line(to: map(176, 140, in: rect))
        path.line(to: map(128, 180, in: rect))
        path.line(to: map(128, 112, in: rect))
        path.close()
        return path
    }


    private static func leafStroke(in rect: NSRect) -> NSBezierPath {
        let path = NSBezierPath()
        path.move(to: map(128, 38, in: rect))
        path.line(to: map(176, 72, in: rect))
        path.line(to: map(128, 112, in: rect))
        path.line(to: map(80, 72, in: rect))
        path.close()
        path.move(to: map(80, 72, in: rect))
        path.line(to: map(80, 140, in: rect))
        path.line(to: map(128, 180, in: rect))
        path.line(to: map(176, 140, in: rect))
        path.line(to: map(176, 72, in: rect))
        path.move(to: map(128, 112, in: rect))
        path.line(to: map(128, 180, in: rect))
        return path
    }
}
