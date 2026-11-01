import Foundation
import ServiceManagement

/// Launch at login via `SMAppService`, falling back to a user LaunchAgent when the system
/// refuses the registration (possible for self-signed builds).
@MainActor
enum LoginItem {
    enum State: Equatable {
        case enabled
        case enabledViaLaunchAgent
        case requiresApproval
        case disabled
    }

    static var state: State {
        switch SMAppService.mainApp.status {
        case .enabled: return .enabled
        case .requiresApproval: return .requiresApproval
        default: return LaunchAgent.isInstalled ? .enabledViaLaunchAgent : .disabled
        }
    }

    static func setEnabled(_ enabled: Bool) throws {
        if enabled {
            do {
                try SMAppService.mainApp.register()
                LaunchAgent.remove()
            } catch {
                try LaunchAgent.install()
            }
        } else {
            try? SMAppService.mainApp.unregister()
            LaunchAgent.remove()
        }
    }

    static func openSystemSettings() {
        SMAppService.openSystemSettingsLoginItems()
    }
}

/// `~/Library/LaunchAgents/<bundle id>.login.plist` that opens the app at login.
private enum LaunchAgent {
    static var url: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appending(path: "Library/LaunchAgents/\(AppEnvironment.bundleID).login.plist")
    }

    static var isInstalled: Bool { FileManager.default.fileExists(atPath: url.path) }

    static func install() throws {
        let plist: [String: Any] = [
            "Label": "\(AppEnvironment.bundleID).login",
            "ProgramArguments": ["/usr/bin/open", "-b", AppEnvironment.bundleID],
            "RunAtLoad": true,
            "LimitLoadToSessionType": "Aqua",
        ]
        let data = try PropertyListSerialization.data(fromPropertyList: plist, format: .xml, options: 0)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try data.write(to: url, options: .atomic)
    }

    static func remove() {
        try? FileManager.default.removeItem(at: url)
    }
}
