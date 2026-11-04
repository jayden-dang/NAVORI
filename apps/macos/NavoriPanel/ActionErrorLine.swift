/// The one red caption under the Integrations rows.
///
/// An Action failure stays until the next Action succeeds. The status read that follows every
/// Action must not clear it, or the failure is never seen. A status failure is cleared by the
/// next status read that works.
public struct ActionErrorLine: Equatable, Sendable {
    private enum Source { case action, status }

    public private(set) var message: String?
    private var source: Source?

    public init() {}

    public mutating func actionFailed(_ message: String) {
        self.message = message
        source = .action
    }

    public mutating func actionSucceeded() {
        message = nil
        source = nil
    }

    public mutating func statusFailed(_ message: String) {
        self.message = message
        source = .status
    }

    public mutating func statusLoaded() {
        guard source == .status else { return }
        message = nil
        source = nil
    }
}
