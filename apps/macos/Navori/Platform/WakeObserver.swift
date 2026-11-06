import AppKit

@MainActor
final class WakeObserver {
    private let token: NSObjectProtocol
    private let sleepToken: NSObjectProtocol

    init(onWake: @escaping @MainActor () -> Void, onSleep: @escaping @MainActor () -> Void) {
        token = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification,
            object: nil,
            queue: .main
        ) { _ in
            MainActor.assumeIsolated {
                onWake()
            }
        }
        sleepToken = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.willSleepNotification,
            object: nil,
            queue: .main
        ) { _ in
            MainActor.assumeIsolated {
                onSleep()
            }
        }
    }

    isolated deinit {
        NSWorkspace.shared.notificationCenter.removeObserver(token)
        NSWorkspace.shared.notificationCenter.removeObserver(sleepToken)
    }
}
