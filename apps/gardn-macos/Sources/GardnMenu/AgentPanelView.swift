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
                        Circle()
                            .fill(statusColor(agent.status))
                            .frame(width: 7, height: 7)
                        VStack(alignment: .leading, spacing: 1) {
                            Text(agent.title)
                                .font(.callout)
                                .foregroundStyle(.primary)
                                .lineLimit(1)
                            if !agent.subtitle.isEmpty {
                                Text(agent.subtitle)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                        }
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 8)
                    .padding(.vertical, 6)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .background(
                    RoundedRectangle(cornerRadius: 6)
                        .fill(Color.primary.opacity(0.04))
                )
            }
        }
    }

    private func sectionColor(_ section: AgentRecord.Section) -> Color {
        switch section {
        case .triage:
            return Color(red: 0.72, green: 0.42, blue: 0.18)
        case .working:
            return Color(red: 0.11, green: 0.35, blue: 0.24)
        case .idle:
            return Color.secondary
        }
    }

    private func statusColor(_ status: AgentRecord.Status) -> Color {
        switch status {
        case .blocked, .done:
            return Color(red: 0.72, green: 0.42, blue: 0.18)
        case .working:
            return Color(red: 0.49, green: 0.73, blue: 0.45)
        case .idle, .unknown:
            return Color.secondary
        }
    }
}
