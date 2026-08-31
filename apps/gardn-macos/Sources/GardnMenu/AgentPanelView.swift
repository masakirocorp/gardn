import SwiftUI

struct AgentPanelView: View {
    @ObservedObject var store: AgentStore

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider()
            if !store.connected {
                disconnected
            } else if store.agents.isEmpty {
                empty
            } else {
                list
            }
        }
        .frame(width: 320, height: 420)
        .background(Color(nsColor: NSColor.windowBackgroundColor))
    }

    private var header: some View {
        HStack {
            Text("Agents")
                .font(.headline)
            Spacer()
            if store.connected {
                Text("\(store.agents.count)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
    }

    private var disconnected: some View {
        VStack(spacing: 8) {
            Spacer()
            Text(store.connectionMessage ?? "Gardn isn’t running")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 16)
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    private var empty: some View {
        VStack {
            Spacer()
            Text("No agents")
                .foregroundStyle(.secondary)
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    private var list: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                ForEach(AgentRecord.Section.allCases, id: \.self) { section in
                    let rows = store.agents(in: section)
                    if !rows.isEmpty {
                        sectionBlock(section, rows: rows)
                    }
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
        }
    }

    private func sectionBlock(_ section: AgentRecord.Section, rows: [AgentRecord]) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(section.rawValue.uppercased())
                .font(.caption2)
                .fontWeight(.semibold)
                .foregroundStyle(sectionColor(section))
                .padding(.leading, 4)
            ForEach(rows) { agent in
                Button {
                    store.focus(agent)
                } label: {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        titleLabel(agent.title)
                        Spacer(minLength: 4)
                        if !agent.subtitle.isEmpty {
                            Text(agent.subtitle)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                    .padding(.horizontal, 8)
                    .padding(.vertical, 6)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func titleLabel(_ title: String) -> some View {
        let prefix: String
        let leaf: String
        if let idx = title.lastIndex(of: "/") {
            prefix = String(title[...idx])
            leaf = String(title[title.index(after: idx)...])
        } else {
            prefix = ""
            leaf = title
        }
        return HStack(spacing: 0) {
            if !prefix.isEmpty {
                Text(prefix)
                    .foregroundStyle(.tertiary)
            }
            Text(leaf)
                .foregroundStyle(.primary)
        }
        .font(.callout)
        .lineLimit(1)
    }

    private func sectionColor(_ section: AgentRecord.Section) -> Color {
        switch section {
        case .triage:
            return Color(red: 1.0, green: 0.72, blue: 0.42)
        case .working:
            return Color(red: 0.95, green: 0.82, blue: 0.22)
        case .idle:
            return Color(red: 0.49, green: 0.73, blue: 0.45)
        }
    }
}
