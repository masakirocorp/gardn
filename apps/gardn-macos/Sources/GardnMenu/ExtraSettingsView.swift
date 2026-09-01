import SwiftUI

struct ExtraSettingsView: View {
    @ObservedObject var store: AgentStore
    @ObservedObject var catalog: CoordinatorCatalog
    @State private var remoteTarget = ""

    var body: some View {
        TabView {
            servers
                .tabItem {
                    Label("Servers", systemImage: "externaldrive.connected.to.line.below")
                }
            ExtraAboutView()
                .tabItem {
                    Label("About", systemImage: "info.circle")
                }
        }
        .frame(minWidth: 560, minHeight: 420)
    }

    private var servers: some View {
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
        .navigationTitle("Servers")
    }
}

struct ExtraAboutView: View {
    var body: some View {
        VStack(spacing: 18) {
            Image("Logo")
                .resizable()
                .interpolation(.high)
                .frame(width: 96, height: 96)
            Text("Gardn")
                .font(.system(size: 22, weight: .semibold))
            Text("Menu extra \(version)")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
            HStack(spacing: 16) {
                Link("Website", destination: URL(string: "https://gardn.dev")!)
                Link("GitHub", destination: URL(string: "https://github.com/masakirocorp/gardn")!)
            }
            .font(.system(size: 13))
            Button("Check for Updates") {
                ExtraAppDelegate.updaterController.updater.checkForUpdates()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(32)
        .navigationTitle("About")
    }

    private var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
            ?? "0.1.0"
    }
}
