import AppKit

@MainActor
final class WakeObserver {
    private let token: NSObjectProtocol

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
        NSWorkspace.shared.notificationCenter.removeObserver(token)
    }
}
