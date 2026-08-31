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
    private let statusItem = NSStatusBar.system.statusItem(withLength: 22)
    private let popover = NSPopover()

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        popover.behavior = .transient
        popover.animates = false
        popover.contentViewController = NSHostingController(
            rootView: AgentPanelView(store: store)
        )
        popover.contentSize = NSSize(width: 268, height: 420)
        statusItem.button?.imagePosition = .imageOnly
        statusItem.button?.action = #selector(togglePopover)
        statusItem.button?.target = self
        store.onNeedsAttentionChange = { [weak self] alert in
            self?.applyIcon(alert)
        }
        applyIcon(store.needsAttention)
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
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
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
