import AppKit
import SwiftUI

struct AgentPanelView: View {
    @ObservedObject var store: AgentStore

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            if let actionError = store.actionError {
                Text(actionError)
                    .font(.system(size: 11))
                    .foregroundStyle(.red)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 6)
            }
            if !store.connected {
                disconnected
            } else if store.agents.isEmpty {
                empty
            } else {
                list
            }
        }
        .frame(width: 276, height: panelHeight, alignment: .top)
        .background { PopoverChrome().ignoresSafeArea() }
    }

    private var panelHeight: CGFloat {
        if !store.connected || store.agents.isEmpty {
            return 92
        }
        let filled = AgentRecord.Section.allCases.filter { !store.agents(in: $0).isEmpty }.count
        let followUpEmpty = store.agents(in: .followUp).isEmpty
        let sections = filled + (followUpEmpty ? 1 : 0)
        let height = 40 + CGFloat(sections) * 28 + CGFloat(store.agents.count) * 30 + (followUpEmpty ? 22 : 0) + (store.actionError == nil ? 0 : 22) + 12
        return min(560, height)
    }

    private var header: some View {
        HStack(alignment: .firstTextBaseline) {
            Text("Agents")
                .font(.system(size: 13, weight: .semibold))
            Spacer()
            if store.connected, !store.agents.isEmpty {
                Text("\(store.agents.count)")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.tertiary)
                    .monospacedDigit()
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 10)
        .padding(.bottom, 8)
    }

    private var disconnected: some View {
        Text(store.connectionMessage ?? "Gardn isn’t running")
            .font(.system(size: 12))
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.bottom, 12)
    }

    private var empty: some View {
        Text("No agents")
            .font(.system(size: 12))
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.bottom, 12)
    }

    private var list: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 2) {
                ForEach(AgentRecord.Section.allCases, id: \.self) { section in
                    let rows = store.agents(in: section)
                    if section == .followUp || !rows.isEmpty {
                        sectionBlock(section, rows: rows)
                    }
                }
            }
            .padding(.horizontal, 6)
            .padding(.bottom, 8)
        }
    }

    private func sectionBlock(_ section: AgentRecord.Section, rows: [AgentRecord]) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(section.rawValue.uppercased())
                .font(.system(size: 10, weight: .semibold))
                .tracking(0.6)
                .foregroundStyle(sectionColor(section))
                .padding(.horizontal, 8)
                .padding(.top, 8)
                .padding(.bottom, 3)
            ForEach(rows) { agent in
                AgentRow(
                    agent: agent,
                    onFocus: { store.focus(agent) },
                    onFollowUp: { store.setFollowUp(agent, enabled: $0) }
                )
            }
            if section == .followUp, rows.isEmpty {
                Text("Right-click an agent to add")
                    .font(.system(size: 12))
                    .foregroundStyle(.tertiary)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
            }
        }
    }

    private func sectionColor(_ section: AgentRecord.Section) -> Color {
        switch section {
        case .triage:
            return Color(red: 1.00, green: 0.72, blue: 0.42)
        case .followUp:
            return Color(red: 0.78, green: 0.52, blue: 0.86)
        case .working:
            return Color(red: 0.90, green: 0.76, blue: 0.22)
        case .idle:
            return Color(red: 0.42, green: 0.73, blue: 0.48)
        }
    }
}

private struct AgentRow: View {
    var agent: AgentRecord
    var onFocus: () -> Void
    var onFollowUp: (Bool) -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: onFocus) {
            HStack(spacing: 8) {
                titleLabel(agent.title)
                Spacer(minLength: 8)
                if !agent.subtitle.isEmpty {
                    Text(agent.subtitle)
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(hovering ? Color.primary.opacity(0.08) : Color.clear)
            )
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .contextMenu {
            if agent.followUp {
                Button("Remove from Follow Up") { onFollowUp(false) }
            } else {
                Button("Add to Follow Up") { onFollowUp(true) }
            }
        }
    }

    private func titleLabel(_ title: String) -> some View {
        let parts = splitTitle(title)
        return HStack(spacing: 0) {
            if !parts.prefix.isEmpty {
                Text(parts.prefix)
                    .foregroundStyle(.tertiary)
            }
            Text(parts.leaf)
                .foregroundStyle(.primary)
        }
        .font(.system(size: 13))
        .lineLimit(1)
    }

    private func splitTitle(_ title: String) -> (prefix: String, leaf: String) {
        guard let idx = title.lastIndex(of: "/") else {
            return ("", title)
        }
        return (String(title[...idx]), String(title[title.index(after: idx)...]))
    }
}

private struct PopoverChrome: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = .hudWindow
        view.blendingMode = .behindWindow
        view.state = .active
        view.isEmphasized = true
        return view
    }

    func updateNSView(_ view: NSVisualEffectView, context: Context) {
        guard let window = view.window else { return }
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.styleMask.insert(.fullSizeContentView)
    }
}
