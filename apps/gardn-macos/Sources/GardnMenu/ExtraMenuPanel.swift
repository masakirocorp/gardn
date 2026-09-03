import AppKit
import SwiftUI

@MainActor
final class ExtraMenuPanel {
    let panel: NSPanel
    private let hosting: NSHostingController<AgentPanelView>
    private weak var statusItem: NSStatusItem?
    private var localMonitor: Any?
    private var globalMonitor: Any?
    private var frameObserver: NSObjectProtocol?

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
        panel.hidesOnDeactivate = true
        panel.becomesKeyOnlyIfNeeded = true
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.transient, .ignoresCycle, .fullScreenAuxiliary]
        let glass = NSGlassEffectView()
        glass.style = .regular
        glass.cornerRadius = 12
        glass.contentView = hosting.view
        hosting.view.wantsLayer = true
        hosting.view.layer?.backgroundColor = NSColor.clear.cgColor
        hosting.view.postsFrameChangedNotifications = true
        panel.contentView = glass
        self.panel = panel
        frameObserver = NotificationCenter.default.addObserver(
            forName: NSView.frameDidChangeNotification,
            object: hosting.view,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.syncSizeIfShown()
            }
        }
    }


    func attach(statusItem: NSStatusItem) {
        self.statusItem = statusItem
    }

    func show() {
        guard let button = statusItem?.button else { return }
        syncSize()
        position(relativeTo: button)
        panel.orderFrontRegardless()
        installMonitors()
    }

    func hide() {
        removeMonitors()
        panel.orderOut(nil)
    }

    private func syncSizeIfShown() {
        guard isShown, let button = statusItem?.button else { return }
        syncSize()
        position(relativeTo: button)
    }

    private func syncSize() {
        hosting.view.layoutSubtreeIfNeeded()
        let size = hosting.view.fittingSize
        if size.width > 0, size.height > 0 {
            panel.setContentSize(size)
        }
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
        localMonitor = NSEvent.addLocalMonitorForEvents(matching: [
            .leftMouseDown, .rightMouseDown, .keyDown,
        ]) { [weak self] event in
            guard let self else { return event }
            if event.type == .keyDown {
                if event.keyCode == 53 {
                    self.hide()
                    return nil
                }
                return event
            }
            if self.hitsPanel(event) || self.hitsStatusItem(event) {
                return event
            }
            self.hide()
            return event
        }
        globalMonitor = NSEvent.addGlobalMonitorForEvents(matching: [
            .leftMouseDown, .rightMouseDown,
        ]) { [weak self] _ in
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
