import AppKit

@MainActor
final class WakeObserver {
    private var token: NSObjectProtocol?

    init(onWake: @escaping @MainActor () -> Void) {
        token = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification,
            object: nil,
            queue: .main
        ) { _ in
            MainActor.assumeIsolated {
                onWake()
            }
        }
    }

    isolated deinit {
        if let token {
            NSWorkspace.shared.notificationCenter.removeObserver(token)
        }
    }
}
