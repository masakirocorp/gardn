import AppKit
import SwiftUI

struct AgentPanelView: View {
    @ObservedObject var store: AgentStore
    @ObservedObject var catalog: CoordinatorCatalog
    @State private var addingRemote = false
    @State private var remoteTarget = ""
    @State private var remoteSession = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            if let actionError = store.actionError {
                Text(actionError)
                    .font(.system(size: 10))
                    .foregroundStyle(.red)
                    .padding(.horizontal, 10)
                    .padding(.bottom, 4)
            }
            if !store.connected {
                disconnected
            } else if store.agents.isEmpty {
                empty
            } else {
                list
            }
        }
        .frame(width: 268, height: panelHeight, alignment: .top)
        .background { PopoverChrome().ignoresSafeArea() }
        .sheet(isPresented: $addingRemote) { addRemote }
    }

    private var panelHeight: CGFloat {
        if !store.connected || store.agents.isEmpty {
            return 80
        }
        var height: CGFloat = 36
        if store.actionError != nil { height += 18 }
        var firstSection = true
        for section in AgentRecord.Section.allCases {
            let rows = store.agents(in: section)
            if section != .followUp, rows.isEmpty { continue }
            if !firstSection { height += 6 }
            firstSection = false
            height += 24
            if store.isCollapsed(section) { continue }
            if rows.isEmpty {
                height += 16
            } else {
                height += CGFloat(rows.count) * 34
            }
        }
        return min(560, height + 8)
    }


    private var header: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text("Agents")
                .font(.system(size: 12, weight: .semibold))
            Spacer(minLength: 8)
            Menu {
                ForEach(catalog.coordinators) { coordinator in
                    Button {
                        store.selectCoordinator(coordinator.id)
                    } label: {
                        if coordinator.id == catalog.selectedId {
                            Label(coordinator.title, systemImage: "checkmark")
                        } else {
                            Text(coordinator.title)
                        }
                    }
                }
                Divider()
                Button("Add Remote Server…") {
                    remoteTarget = ""
                    remoteSession = ""
                    addingRemote = true
                }
            } label: {
                Text(catalog.selected?.title ?? "Server")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
        }
        .padding(.horizontal, 10)
        .padding(.top, 8)
        .padding(.bottom, 4)
    }

    private var addRemote: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Add Remote Server")
                .font(.system(size: 13, weight: .semibold))
            TextField("SSH target", text: $remoteTarget)
            TextField("Session (optional)", text: $remoteSession)
            if let addError = catalog.addError {
                Text(addError)
                    .font(.system(size: 11))
                    .foregroundStyle(.red)
            }
            HStack {
                Spacer()
                Button("Cancel") { addingRemote = false }
                Button("Add") {
                    store.addRemoteCoordinator(target: remoteTarget, session: remoteSession)
                    if catalog.addError == nil {
                        addingRemote = false
                    }
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(16)
        .frame(width: 280)
    }


    private var disconnected: some View {
        Text(store.connectionMessage ?? "\(catalog.selected?.title ?? "Gardn") isn’t running")
            .font(.system(size: 11))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 10)
            .padding(.bottom, 10)
    }

    private var empty: some View {
        Text("No agents")
            .font(.system(size: 11))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 10)
            .padding(.bottom, 10)
    }

    private var list: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 6) {
                ForEach(AgentRecord.Section.allCases, id: \.self) { section in
                    let rows = store.agents(in: section)
                    if section == .followUp || !rows.isEmpty {
                        sectionBlock(section, rows: rows)
                    }
                }
            }
            .padding(.bottom, 6)
        }
    }

    private func sectionBlock(_ section: AgentRecord.Section, rows: [AgentRecord]) -> some View {
        let collapsed = store.isCollapsed(section)
        return VStack(alignment: .leading, spacing: 0) {
            SectionHeader(
                section: section,
                icon: sectionIcon(section),
                color: sectionColor(section),
                collapsed: collapsed,
                count: rows.count
            ) {
                store.toggleCollapsed(section)
            }
            if !collapsed {
                ForEach(rows) { agent in
                    AgentRow(
                        agent: agent,
                        onFocus: { store.focus(agent) },
                        onFollowUp: { store.setFollowUp(agent, enabled: $0) }
                    )
                    .padding(.leading, 20)
                    .padding(.trailing, 8)
                }
                if section == .followUp, rows.isEmpty {
                    Text("None")
                        .font(.system(size: 11))
                        .foregroundStyle(.tertiary)
                        .padding(.leading, 20)
                        .padding(.trailing, 8)
                        .padding(.vertical, 2)
                }

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

private struct SectionHeader: View {
    var section: AgentRecord.Section
    var icon: String
    var color: Color
    var collapsed: Bool
    var count: Int
    var onToggle: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: onToggle) {
            HStack(spacing: 5) {
                Text(icon)
                Text(section.rawValue)
                Spacer(minLength: 0)
                if collapsed, count > 0 {
                    Text("\(count)")
                        .font(.system(size: 11, weight: .semibold).monospacedDigit())
                }
            }
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(color.opacity(collapsed ? 0.55 : 1))
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .background(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(hovering ? Color(nsColor: .quaternarySystemFill) : .clear)
            )

        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
    }
}


private struct AgentRow: View {
    var agent: AgentRecord
    var onFocus: () -> Void
    var onFollowUp: (Bool) -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: onFocus) {
            VStack(alignment: .leading, spacing: 0) {
                titleLabel(agent.title)
                    .font(.system(size: 12, weight: agent.focused ? .semibold : .regular))
                metaLine
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .background(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
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
        hovering ? Color(nsColor: .quaternarySystemFill) : .clear
    }


    private var metaLine: some View {
        HStack(spacing: 0) {
            if let group = agent.groupName, !group.isEmpty {
                Text(group)
                    .foregroundStyle(accentColor(agent.groupAccent))
            }
            if agent.showsStatus, let status = agent.statusLabel {
                if agent.groupName != nil { Text(" · ").foregroundStyle(.tertiary) }
                Text(status).foregroundStyle(statusColor(agent.status))
            }
            if let age = agent.age {
                if agent.groupName != nil || agent.showsStatus {
                    Text(" · ").foregroundStyle(.tertiary)
                }
                Text(age).foregroundStyle(.secondary)
            }
        }
        .font(.system(size: 12))
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
