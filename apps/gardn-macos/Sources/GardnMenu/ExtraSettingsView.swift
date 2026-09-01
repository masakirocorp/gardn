import SwiftUI

struct ExtraSettingsView: View {
    @ObservedObject var store: AgentStore
    @ObservedObject var catalog: CoordinatorCatalog
    @State private var remoteTarget = ""

    var body: some View {
        Form {
            Section {
                ForEach(catalog.coordinators) { coordinator in
                    HStack(alignment: .firstTextBaseline, spacing: 10) {
                        Button {
                            store.selectCoordinator(coordinator.id)
                        } label: {
                            HStack(alignment: .firstTextBaseline, spacing: 8) {
                                Image(
                                    systemName: coordinator.id == catalog.selectedId
                                        ? "checkmark.circle.fill" : "circle"
                                )
                                .foregroundStyle(
                                    coordinator.id == catalog.selectedId
                                        ? Color.accentColor : Color.secondary
                                )
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(coordinator.title)
                                    Text(coordinator.subtitle)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                        .buttonStyle(.plain)
                        Spacer(minLength: 8)
                        if coordinator.kind == .remote {
                            Button("Remove", role: .destructive) {
                                catalog.removeRemote(coordinator.id)
                                store.selectCoordinator(catalog.selectedId)
                            }
                            .buttonStyle(.borderless)
                        }
                    }
                }
            } header: {
                Text("Servers")
            } footer: {
                Text("The extra watches one of these Gardn servers.")
            }
            Section("Add Remote Server") {
                TextField("SSH target", text: $remoteTarget)
                    .textFieldStyle(.roundedBorder)
                if let addError = catalog.addError {
                    Text(addError)
                        .foregroundStyle(.red)
                }
                HStack {
                    Spacer()
                    Button("Add") {
                        store.addRemoteCoordinator(target: remoteTarget, session: "")
                        if catalog.addError == nil {
                            remoteTarget = ""
                        }
                    }
                    .disabled(remoteTarget.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    .keyboardShortcut(.defaultAction)
                }
            }
        }
        .formStyle(.grouped)
        .frame(minWidth: 520, minHeight: 380)
        .navigationTitle("Servers")
    }
}
