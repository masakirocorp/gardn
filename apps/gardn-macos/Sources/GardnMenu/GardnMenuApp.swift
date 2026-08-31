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
                .onAppear { store.start() }
                .onDisappear { store.stop() }
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

private enum StatusItemImage {
    static func load(alert: Bool) -> NSImage? {
        let base = alert ? "StatusAlertTemplate" : "StatusTemplate"
        guard let image = loadPNG(base) ?? loadPNG(base + "@2x") else {
            return nil
        }
        image.size = NSSize(width: 22, height: 22)
        image.isTemplate = true
        return image
    }

    private static func loadPNG(_ name: String) -> NSImage? {
        guard let url = Bundle.module.url(forResource: name, withExtension: "png") else {
            return nil
        }
        return NSImage(contentsOf: url)
    }
}
