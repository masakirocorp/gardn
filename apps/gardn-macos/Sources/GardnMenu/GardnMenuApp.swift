import AppKit
import SwiftUI

@main
struct GardnMenuApp: App {
    @StateObject private var store = AgentStore()

    init() {
        NSApplication.shared.setActivationPolicy(.accessory)
    }

    var body: some Scene {
        MenuBarExtra {
            AgentPanelView(store: store)
        } label: {
            StatusItemLabel(alert: store.needsAttention)
                .id(store.needsAttention)
        }
        .menuBarExtraStyle(.window)
    }
}

private struct StatusItemLabel: View {
    var alert: Bool

    var body: some View {
        if let image = StatusItemImage.load(alert: alert) {
            Image(nsImage: image)
                .renderingMode(.template)
        } else {
            Image(systemName: alert ? "leaf.fill" : "leaf")
                .font(.system(size: 15, weight: .regular))
        }
    }
}

enum StatusItemImage {
    static func load(alert: Bool) -> NSImage? {
        let base = alert ? "StatusAlertTemplate" : "StatusTemplate"
        guard let image = loadPNG(base) ?? loadPNG(base + "@2x") else {
            return nil
        }
        image.size = NSSize(width: 22, height: 22)
        image.isTemplate = true
        return image
    }

    static func applyToStatusItem(alert: Bool) {
        guard let image = load(alert: alert) else { return }
        for button in statusBarButtons() {
            button.image = image
            button.image?.isTemplate = true
        }
    }

    private static func loadPNG(_ name: String) -> NSImage? {
        guard let url = Bundle.module.url(forResource: name, withExtension: "png") else {
            return nil
        }
        return NSImage(contentsOf: url)
    }

    private static func statusBarButtons() -> [NSStatusBarButton] {
        var buttons: [NSStatusBarButton] = []
        for window in NSApp.windows {
            collectButtons(from: window.contentView, into: &buttons)
            collectButtons(from: window.contentView?.superview, into: &buttons)
        }
        return buttons
    }

    private static func collectButtons(from view: NSView?, into buttons: inout [NSStatusBarButton]) {
        guard let view else { return }
        if let button = view as? NSStatusBarButton {
            buttons.append(button)
        }
        for subview in view.subviews {
            collectButtons(from: subview, into: &buttons)
        }
    }
}
