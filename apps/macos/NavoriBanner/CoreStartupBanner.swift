import Foundation
import NavoriCore

/// Formats core start failures for the menubar failed-start banner.
public enum CoreStartupBanner {
    /// Matches Rust `CoreError` Display (`startup failed: …`).
    public static func message(for error: Error) -> String {
        guard let error = error as? CoreError else {
            return error.localizedDescription
        }
        switch error {
        case .Startup(let message):
            return "startup failed: \(message)"
        case .InvalidJson(let message):
            return "invalid JSON: \(message)"
        case .Dispatch(let message):
            return message
        case .ShutDown:
            return "core is shut down"
        }
    }

    /// Awake menu failure text. A core rejection stores the action sentence, not `String(reflecting:)`.
    public static func menuFailure(for error: Error) -> String {
        message(for: error)
    }
}
