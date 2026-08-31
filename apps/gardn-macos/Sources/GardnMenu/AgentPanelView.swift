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
        .frame(width: 280, height: panelHeight, alignment: .top)
        .background { PopoverChrome().ignoresSafeArea() }
    }

    private var panelHeight: CGFloat {
        if !store.connected || store.agents.isEmpty {
            return 92
        }
        let filled = AgentRecord.Section.allCases.filter { !store.agents(in: $0).isEmpty }.count
        let followUpEmpty = store.agents(in: .followUp).isEmpty
        let sections = filled + (followUpEmpty ? 1 : 0)
        let height = 44 + CGFloat(sections) * 26 + CGFloat(store.agents.count) * 38 + (followUpEmpty ? 22 : 0) + (store.actionError == nil ? 0 : 22) + 10
        return min(580, height)
    }

    private var header: some View {
        HStack {
            Text("Agents")
                .font(.system(size: 13, weight: .semibold))
            Spacer()
            Text("All")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 7)
                .padding(.vertical, 2)
                .background(
                    RoundedRectangle(cornerRadius: 4, style: .continuous)
                        .fill(Color.primary.opacity(0.08))
                )
        }
        .padding(.horizontal, 12)
        .padding(.top, 10)
        .padding(.bottom, 6)
    }

    private var disconnected: some View {
        Text(store.connectionMessage ?? "Gardn isn’t running")
            .font(.system(size: 12))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 12)
            .padding(.bottom, 12)
    }

    private var empty: some View {
        Text("No agents")
            .font(.system(size: 12))
            .foregroundStyle(.secondary)
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
            HStack(spacing: 6) {
                Text(sectionIcon(section))
                Text(section.rawValue)
            }
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(sectionColor(section))
            .padding(.horizontal, 8)
            .padding(.top, 8)
            .padding(.bottom, 2)
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

    private func sectionIcon(_ section: AgentRecord.Section) -> String {
        switch section {
        case .triage: return "!"
        case .followUp: return "*"
        case .working: return ":"
        case .idle: return "✓"
        }
    }

    private func sectionColor(_ section: AgentRecord.Section) -> Color {
        switch section {
        case .triage: return Color(red: 1.00, green: 0.72, blue: 0.42)
        case .followUp: return Color(red: 0.70, green: 0.47, blue: 0.85)
        case .working: return Color(red: 0.86, green: 0.56, blue: 0.18)
        case .idle: return Color(red: 0.38, green: 0.68, blue: 0.42)
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
            VStack(alignment: .leading, spacing: 1) {
                titleLabel(agent.title)
                    .font(.system(size: 13, weight: agent.focused ? .semibold : .regular))
                metaLine
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .background(
                RoundedRectangle(cornerRadius: 5, style: .continuous)
                    .fill(rowFill)
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

    private var rowFill: Color {
        if agent.focused {
            return Color.accentColor.opacity(0.22)
        }
        if hovering {
            return Color.accentColor.opacity(0.14)
        }
        return .clear
    }

    private var metaLine: some View {
        HStack(spacing: 0) {
            if let group = agent.groupName, !group.isEmpty {
                Text(group)
                    .foregroundStyle(accentColor(agent.groupAccent))
            }
            if agent.showsStatus, let status = agent.statusLabel {
                if agent.groupName != nil { Text(" - ").foregroundStyle(.tertiary) }
                Text(status).foregroundStyle(statusColor(agent.status))
            }
            if let age = agent.age {
                if agent.groupName != nil || agent.showsStatus {
                    Text(" - ").foregroundStyle(.tertiary)
                }
                Text(age).foregroundStyle(.secondary)
            }
        }
        .font(.system(size: 11))
        .lineLimit(1)
    }

    private func titleLabel(_ title: String) -> some View {
        let parts = splitTitle(title)
        return HStack(spacing: 0) {
            if !parts.prefix.isEmpty {
                Text(parts.prefix).foregroundStyle(.tertiary)
            }
            Text(parts.leaf).foregroundStyle(.primary)
        }
        .lineLimit(1)
    }

    private func splitTitle(_ title: String) -> (prefix: String, leaf: String) {
        guard let idx = title.lastIndex(of: "/") else { return ("", title) }
        return (String(title[...idx]), String(title[title.index(after: idx)...]))
    }

    private func accentColor(_ name: String?) -> Color {
        switch name {
        case "blue": return Color(red: 0.32, green: 0.52, blue: 0.92)
        case "magenta": return Color(red: 0.85, green: 0.42, blue: 0.68)
        case "cyan": return Color(red: 0.28, green: 0.72, blue: 0.82)
        case "green": return Color(red: 0.36, green: 0.68, blue: 0.38)
        case "yellow": return Color(red: 0.88, green: 0.62, blue: 0.20)
        case "red": return Color(red: 0.86, green: 0.36, blue: 0.36)
        default: return Color(red: 0.70, green: 0.47, blue: 0.85)
        }
    }

    private func statusColor(_ status: AgentRecord.Status) -> Color {
        switch status {
        case .blocked: return Color(red: 0.90, green: 0.35, blue: 0.35)
        case .working: return Color(red: 0.86, green: 0.56, blue: 0.18)
        case .done: return Color(red: 0.30, green: 0.68, blue: 0.72)
        case .idle, .unknown: return Color(red: 0.38, green: 0.68, blue: 0.42)
        }
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
