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
final class ExtraAppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
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
    private lazy var menuPanel = ExtraMenuPanel(store: store, catalog: store.catalog)
    private var settingsWindow: NSWindow?

    func applicationDidFinishLaunching(_ notification: Notification) {
        Self.terminateOtherCopies()
        PathCli.installBundledCLI()
        NSApp.setActivationPolicy(.accessory)
        UNUserNotificationCenter.current().delegate = self
        AgentNotifications.requestAuthorization()
        menuPanel.attach(statusItem: statusItem)
        statusItem.button?.imagePosition = .imageOnly
        statusItem.button?.action = #selector(togglePopover)
        statusItem.button?.target = self
        store.onNeedsAttentionChange = { [weak self] alert in
            self?.applyIcon(alert)
        }
        store.onDidFocus = { [weak self] in
            self?.menuPanel.hide()
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
        if menuPanel.isShown {
            menuPanel.hide()
        } else {
            NSApp.activate(ignoringOtherApps: true)
            store.refresh()
            menuPanel.show()
        }
    }

    func openSettings() {
        menuPanel.hide()
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
        let window = settingsWindow ?? makeSettingsWindow()
        settingsWindow = window
        window.makeKeyAndOrderFront(nil)
    }

    private func makeSettingsWindow() -> NSWindow {
        let controller = NSHostingController(
            rootView: ExtraSettingsView(
                store: store,
                catalog: store.catalog,
                checkForUpdates: Self.checkForUpdates
            )
        )
        let window = NSWindow(contentViewController: controller)
        window.title = "Settings"
        window.styleMask = [.titled, .closable, .miniaturizable, .resizable]
        window.setContentSize(NSSize(width: 560, height: 420))
        window.minSize = NSSize(width: 560, height: 420)
        window.isReleasedWhenClosed = false
        window.delegate = self
        window.center()
        return window
    }

    func windowWillClose(_ notification: Notification) {
        guard (notification.object as AnyObject?) === settingsWindow else { return }
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            let titledVisible = NSApp.windows.contains {
                $0.isVisible && $0.styleMask.contains(.titled) && $0 !== self.settingsWindow
            }
            if !titledVisible {
                NSApp.setActivationPolicy(.accessory)
            }
        }
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

@MainActor
private final class ExtraMenuPanel {
    let panel: NSPanel
    private let hosting: NSHostingController<AgentPanelView>
    private weak var statusItem: NSStatusItem?
    private var localMonitor: Any?
    private var globalMonitor: Any?

    var isShown: Bool { panel.isVisible }

    init(store: AgentStore, catalog: CoordinatorCatalog) {
        let hosting = NSHostingController(
            rootView: AgentPanelView(store: store, catalog: catalog)
        )
        hosting.sizingOptions = .preferredContentSize
        self.hosting = hosting
        let panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 268, height: 80),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.isFloatingPanel = true
        panel.level = .popUpMenu
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.hidesOnDeactivate = false
        panel.becomesKeyOnlyIfNeeded = true
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.transient, .ignoresCycle, .fullScreenAuxiliary]
        panel.contentViewController = hosting
        if let content = panel.contentView {
            content.wantsLayer = true
            content.layer?.cornerRadius = 10
            content.layer?.cornerCurve = .continuous
            content.layer?.masksToBounds = true
        }
        self.panel = panel
    }

    func attach(statusItem: NSStatusItem) {
        self.statusItem = statusItem
    }

    func show() {
        guard let button = statusItem?.button else { return }
        hosting.view.layoutSubtreeIfNeeded()
        let size = hosting.view.fittingSize
        if size.width > 0, size.height > 0 {
            panel.setContentSize(size)
        }
        position(relativeTo: button)
        panel.orderFrontRegardless()
        installMonitors()
    }

    func hide() {
        removeMonitors()
        panel.orderOut(nil)
    }

    private func position(relativeTo button: NSStatusBarButton) {
        guard let buttonWindow = button.window else { return }
        let buttonScreen = buttonWindow.convertToScreen(button.convert(button.bounds, to: nil))
        let size = panel.frame.size
        var x = buttonScreen.minX
        let y = buttonScreen.minY - size.height - 5
        if let visible = (buttonWindow.screen ?? NSScreen.main)?.visibleFrame {
            if x + size.width > visible.maxX - 8 {
                x = max(visible.minX + 8, visible.maxX - size.width - 8)
            }
            if x < visible.minX + 8 {
                x = visible.minX + 8
            }
        }
        panel.setFrameOrigin(NSPoint(x: x, y: y))
    }

    private func installMonitors() {
        removeMonitors()
        localMonitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) {
            [weak self] event in
            guard let self else { return event }
            if self.hitsPanel(event) || self.hitsStatusItem(event) {
                return event
            }
            self.hide()
            return event
        }
        globalMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) {
            [weak self] _ in
            DispatchQueue.main.async {
                self?.hide()
            }
        }
    }

    private func removeMonitors() {
        if let localMonitor {
            NSEvent.removeMonitor(localMonitor)
        }
        if let globalMonitor {
            NSEvent.removeMonitor(globalMonitor)
        }
        localMonitor = nil
        globalMonitor = nil
    }

    private func hitsPanel(_ event: NSEvent) -> Bool {
        event.window === panel
    }

    private func hitsStatusItem(_ event: NSEvent) -> Bool {
        guard let button = statusItem?.button, let window = button.window else { return false }
        guard event.window === window else { return false }
        let location = button.convert(event.locationInWindow, from: nil)
        return button.bounds.contains(location)
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
