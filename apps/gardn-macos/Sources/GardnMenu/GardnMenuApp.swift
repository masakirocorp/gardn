import AppKit
import SwiftUI

@main
struct GardnMenuApp: App {
    @NSApplicationDelegateAdaptor(ExtraAppDelegate.self) private var delegate

    var body: some Scene {
        Settings {
            EmptyView()
        }
    }
}

@MainActor
final class ExtraAppDelegate: NSObject, NSApplicationDelegate {
    let store = AgentStore()
    private let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private let popover = NSPopover()

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        popover.behavior = .transient
        popover.animates = false
        popover.contentViewController = NSHostingController(
            rootView: AgentPanelView(store: store)
        )
        popover.contentSize = NSSize(width: 268, height: 420)
        StatusItemImage.button = statusItem.button
        statusItem.button?.imagePosition = .imageOnly
        statusItem.button?.action = #selector(togglePopover)
        statusItem.button?.target = self
        StatusItemImage.apply(alert: store.needsAttention)
    }

    @objc private func togglePopover(_ sender: Any?) {
        guard let button = statusItem.button else { return }
        if popover.isShown {
            popover.performClose(sender)
        } else {
            NSApp.activate(ignoringOtherApps: true)
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        }
    }
}

enum StatusItemImage {
    static weak var button: NSStatusBarButton?

    static func load(alert: Bool) -> NSImage? {
        let base = alert ? "StatusAlertTemplate" : "StatusTemplate"
        guard let image = loadPNG(base) ?? loadPNG(base + "@2x") else {
            return nil
        }
        image.size = NSSize(width: 22, height: 22)
        image.isTemplate = true
        return image
    }

    static func apply(alert: Bool) {
        guard let image = load(alert: alert) else { return }
        button?.image = image
        button?.image?.isTemplate = true
    }

    private static func loadPNG(_ name: String) -> NSImage? {
        guard let url = Bundle.module.url(forResource: name, withExtension: "png") else {
            return nil
        }
        return NSImage(contentsOf: url)
    }
}
