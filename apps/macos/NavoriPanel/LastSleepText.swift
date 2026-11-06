import Foundation

public struct LastSleepText: Equatable, Sendable {
    public let value: String
    public let spoken: String

    public init(value: String, spoken: String) {
        self.value = value
        self.spoken = spoken
    }
}

@MainActor
private enum Formats {
    // Shared and re-zoned per call; main-actor isolation makes that exclusive.
    static let clock = make("HH:mm")
    static let shortWeekday = make("EEE")
    static let longWeekday = make("EEEE")
    static let day = make("d MMM")
    static let dayAndYear = make("d MMM yyyy")

    static func make(_ pattern: String) -> DateFormatter {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.dateFormat = pattern
        return formatter
    }

    static func string(_ formatter: DateFormatter, _ date: Date, _ zone: TimeZone) -> String {
        formatter.timeZone = zone
        return formatter.string(from: date)
    }
}

/// How long ago the Mac last slept, by calendar day in the calendar's zone.
@MainActor
public func lastSleepText(ms: Int64, now: Date, calendar: Calendar) -> LastSleepText {
    let slept = Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
    let zone = calendar.timeZone
    let time = Formats.string(Formats.clock, slept, zone)
    let days = calendar.dateComponents(
        [.day], from: calendar.startOfDay(for: slept), to: calendar.startOfDay(for: now)
    ).day ?? 0
    switch days {
    case ...0:
        return LastSleepText(value: time, spoken: "today at \(time)")
    case 1:
        return LastSleepText(value: "Yesterday \(time)", spoken: "yesterday at \(time)")
    case 2..<7:
        return LastSleepText(
            value: "\(Formats.string(Formats.shortWeekday, slept, zone)) \(time)",
            spoken: "\(Formats.string(Formats.longWeekday, slept, zone)) at \(time)"
        )
    default:
        let sameYear = calendar.component(.year, from: slept) == calendar.component(.year, from: now)
        let date = Formats.string(sameYear ? Formats.day : Formats.dayAndYear, slept, zone)
        return LastSleepText(value: date, spoken: "on \(date) at \(time)")
    }
}
