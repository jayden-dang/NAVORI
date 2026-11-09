import CoreGraphics
import Foundation

public enum NeedsYouTab: Equatable, Sendable {
    case all, todos, updates
}

public struct DecideModifiers: OptionSet, Sendable {
    public let rawValue: UInt
    public init(rawValue: UInt) { self.rawValue = rawValue }
    public static let command = DecideModifiers(rawValue: 1 << 0)
}

public enum DecideKey: Equatable, Sendable {
    case primary, secondary

    public static func from(keyCode: UInt16, modifiers: DecideModifiers) -> DecideKey? {
        switch (keyCode, modifiers.contains(.command)) {
        case (36, false), (76, false):
            .primary
        case (51, true):
            .secondary
        default:
            nil
        }
    }
}

public enum NeedsYouList {
    public static func filter<T, U>(
        _ tab: NeedsYouTab,
        todos: [T],
        updates: [U]
    ) -> (todos: [T], updates: [U]) {
        switch tab {
        case .all:
            (todos, updates)
        case .todos:
            (todos, [])
        case .updates:
            ([], updates)
        }
    }

    public static func drawnIds(rowFrames: [Int64: CGRect], viewport: CGRect) -> Set<Int64> {
        Set(rowFrames.compactMap { id, frame in
            frame.intersects(viewport) ? id : nil
        })
    }

    /// Retry on a failed Decide row re-sends the choice that failed, even when the left button is primary.
    public static func answerRequest(
        primary wantsPrimary: Bool,
        failedChoice: String?,
        primaryRequest: String,
        secondaryRequest: String
    ) -> String {
        if wantsPrimary, failedChoice == "secondary" {
            return secondaryRequest
        }
        return wantsPrimary ? primaryRequest : secondaryRequest
    }
}
