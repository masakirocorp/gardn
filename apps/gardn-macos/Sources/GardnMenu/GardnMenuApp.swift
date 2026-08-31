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
        }
        .menuBarExtraStyle(.window)
    }
}

private struct StatusItemLabel: View {
    var alert: Bool

    var body: some View {
        if let image = StatusItemImage.load(alert: alert) {
            Image(nsImage: image)
        } else {
            Image(systemName: alert ? "cube.fill" : "cube")
        }
    }
}

private enum StatusItemImage {
    static func load(alert: Bool) -> NSImage? {
        let name = alert ? "StatusAlertTemplate@2x" : "StatusTemplate@2x"
        guard let url = Bundle.module.url(forResource: name, withExtension: "png") else {
            return nil
        }
        guard let image = NSImage(contentsOf: url) else {
            return nil
        }
        image.size = NSSize(width: 18, height: 18)
        image.isTemplate = true
        return image
    }
}
