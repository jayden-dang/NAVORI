import AppKit
import Observation
import SwiftUI

enum SettingsTab: Hashable {
    case general, diagnostics, about
}

@MainActor
@Observable
final class SettingsSelection {
    var tab: SettingsTab = .general
}

struct SettingsView: View {
    let store: CoreStore
    @Bindable var selection: SettingsSelection

    var body: some View {
        TabView(selection: $selection.tab) {
            GeneralTab(store: store)
                .tabItem { Label("General", systemImage: "gearshape") }
                .tag(SettingsTab.general)
            DiagnosticsTab(store: store)
                .tabItem { Label("Diagnostics", systemImage: "stethoscope") }
                .tag(SettingsTab.diagnostics)
            AboutTab(store: store)
                .tabItem { Label("About", systemImage: "info.circle") }
                .tag(SettingsTab.about)
        }
        .padding(20)
        .frame(width: 560, height: 420)
    }
}

private struct GeneralTab: View {
    let store: CoreStore
    @State private var loginState = LoginItem.state
    @State private var cliInstalled = CLIInstaller.isInstalled
    @State private var error: String?

    var body: some View {
        Form {
            Section("Startup") {
                Toggle("Open \(AppEnvironment.displayName) at login", isOn: Binding(
                    get: { loginState == .enabled || loginState == .enabledViaLaunchAgent },
                    set: { enable in
                        do {
                            try LoginItem.setEnabled(enable)
                            error = nil
                        } catch {
                            self.error = error.localizedDescription
                        }
                        loginState = LoginItem.state
                    }
                ))
                switch loginState {
                case .requiresApproval:
                    HStack {
                        Text("macOS needs your approval in Login Items.").foregroundStyle(.orange)
                        Button("Open Login Items…") { LoginItem.openSystemSettings() }
                    }
                case .enabledViaLaunchAgent:
                    Text("Enabled through a LaunchAgent (System Login Items refused this build's signature).")
                        .font(.caption).foregroundStyle(.secondary)
                default:
                    EmptyView()
                }
            }

            Section("Command-line tool") {
                LabeledContent("Command") {
                    Text(CLIInstaller.command).font(.system(.body, design: .monospaced)).textSelection(.enabled)
                }
                if !AppEnvironment.isDev {
                    HStack {
                        Button(cliInstalled ? "Reinstall in ~/.local/bin" : "Install in ~/.local/bin") {
                            do {
                                try CLIInstaller.install()
                                error = nil
                            } catch {
                                self.error = error.localizedDescription
                            }
                            cliInstalled = CLIInstaller.isInstalled
                        }
                        if cliInstalled {
                            Label("Installed", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
                        }
                    }
                    Text("Make sure ~/.local/bin is on your PATH.").font(.caption).foregroundStyle(.secondary)
                }
            }

            if let error {
                Text(error).foregroundStyle(.red).font(.caption)
            }
        }
        .formStyle(.grouped)
        .onAppear {
            loginState = LoginItem.state
            cliInstalled = CLIInstaller.isInstalled
        }
    }
}

private struct DiagnosticsTab: View {
    let store: CoreStore

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Button(store.isRunningChecks ? "Checking…" : "Run Checks") { store.runChecks() }
                    .disabled(store.isRunningChecks || store.snapshot == nil)
                Spacer()
                Button("Open Logs") { open(store.snapshot?.logsDir) }
                Button("Open Data Folder") { open(store.snapshot?.dataDir) }
                Button("Reveal config.toml") { revealConfig() }
            }
            List(store.checks, id: \.name) { check in
                HStack(alignment: .firstTextBaseline) {
                    Image(systemName: check.ok ? "checkmark.circle.fill" : "xmark.circle.fill")
                        .foregroundStyle(check.ok ? .green : .red)
                    Text(check.name).frame(width: 80, alignment: .leading)
                    Text(check.detail).font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                }
            }
            .overlay {
                if store.checks.isEmpty {
                    Text("Run the checks to verify config, database, IPC, modules, awake and logs.")
                        .foregroundStyle(.secondary)
                }
            }
            Text("Same checks from a terminal: \(CLIInstaller.command) doctor")
                .font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
        }
        .onAppear { if store.checks.isEmpty { store.runChecks() } }
    }

    private func open(_ path: String?) {
        guard let path else { return }
        NSWorkspace.shared.open(URL(filePath: path, directoryHint: .isDirectory))
    }

    private func revealConfig() {
        guard let dir = store.snapshot?.dataDir else { return }
        let config = URL(filePath: dir).appending(path: "config.toml")
        if !FileManager.default.fileExists(atPath: config.path) {
            let template = """
            # Navori configuration. Changes apply on the next launch.
            # log_level = "info"        # error | warn | info | debug | trace

            [journal]
            # retention_days = 30

            """
            try? template.write(to: config, atomically: true, encoding: .utf8)
        }
        NSWorkspace.shared.activateFileViewerSelecting([config])
    }
}

private struct AboutTab: View {
    let store: CoreStore

    var body: some View {
        Form {
            LabeledContent("Version", value: "\(AppEnvironment.version) (\(AppEnvironment.build))")
            LabeledContent("Profile", value: AppEnvironment.profile)
            LabeledContent("Bundle ID", value: AppEnvironment.bundleID)
            if let s = store.snapshot {
                LabeledContent("Core PID", value: String(s.pid))
                LabeledContent("Data", value: s.dataDir)
                LabeledContent("Logs", value: s.logsDir)
                LabeledContent("Socket", value: s.socketPath)
            }
        }
        .formStyle(.grouped)
        .textSelection(.enabled)
    }
}
