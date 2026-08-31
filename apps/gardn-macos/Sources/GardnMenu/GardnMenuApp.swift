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
            Image(systemName: alert ? "cube.fill" : "cube")
                .font(.system(size: 16, weight: .regular))
        }
    }
}

private enum StatusItemImage {
    static func load(alert: Bool) -> NSImage? {
        let base = alert ? "StatusAlertTemplate" : "StatusTemplate"
        guard let source = loadPNG(base) ?? loadPNG(base + "@2x") else {
            return nil
        }
        return fittedTemplate(source)
    }

    private static func loadPNG(_ name: String) -> NSImage? {
        guard let url = Bundle.module.url(forResource: name, withExtension: "png") else {
            return nil
        }
        return NSImage(contentsOf: url)
    }

    private static func fittedTemplate(_ source: NSImage) -> NSImage {
        let point: CGFloat = 22
        guard let cg = source.cgImage(forProposedRect: nil, context: nil, hints: nil),
              let cropped = cropOpaque(cg)
        else {
            source.size = NSSize(width: point, height: point)
            source.isTemplate = true
            return source
        }
        let fitted = NSImage(cgImage: cropped, size: NSSize(width: point, height: point))
        fitted.isTemplate = true
        return fitted
    }

    private static func cropOpaque(_ image: CGImage) -> CGImage? {
        let width = image.width
        let height = image.height
        let bytesPerPixel = 4
        let bytesPerRow = width * bytesPerPixel
        var data = [UInt8](repeating: 0, count: max(1, height * bytesPerRow))
        guard let ctx = CGContext(
            data: &data,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: bytesPerRow,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else {
            return image
        }
        ctx.translateBy(x: 0, y: CGFloat(height))
        ctx.scaleBy(x: 1, y: -1)
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        var minX = width
        var minY = height
        var maxX = 0
        var maxY = 0
        for y in 0..<height {
            for x in 0..<width {
                if data[y * bytesPerRow + x * bytesPerPixel + 3] > 24 {
                    minX = min(minX, x)
                    minY = min(minY, y)
                    maxX = max(maxX, x)
                    maxY = max(maxY, y)
                }
            }
        }
        guard maxX >= minX, maxY >= minY else { return image }
        let pad = 2
        let rect = CGRect(
            x: max(0, minX - pad),
            y: max(0, minY - pad),
            width: min(width, maxX + pad + 1) - max(0, minX - pad),
            height: min(height, maxY + pad + 1) - max(0, minY - pad)
        )
        return image.cropping(to: rect) ?? image
    }
}
