import Foundation

public enum DueStatus: Equatable, Sendable {
    case overdue, today, soon, later, none
}

public enum NeedsYouText {
    private static let day: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.calendar = Calendar(identifier: .gregorian)
        f.timeZone = TimeZone(secondsFromGMT: 0)!
        f.dateFormat = "yyyy-MM-dd"
        return f
    }()

    private static let shortWeekday: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.calendar = Calendar(identifier: .gregorian)
        f.timeZone = TimeZone(secondsFromGMT: 0)!
        f.dateFormat = "EEE"
        return f
    }()

    private static let spokenDay: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.calendar = Calendar(identifier: .gregorian)
        f.timeZone = TimeZone(secondsFromGMT: 0)!
        f.dateFormat = "EEEE d MMMM"
        return f
    }()

    public static func pill(dueOn: String?, today: String) -> String {
        guard let dueOn, let due = day.date(from: dueOn), let start = day.date(from: today) else {
            return "No date"
        }
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 0)!
        let days = calendar.dateComponents([.day], from: start, to: due).day ?? 0
        switch days {
        case 0:
            return "Today"
        case 1:
            return "Tomorrow"
        case 2...6:
            return shortWeekday.string(from: due)
        default:
            let wkd = shortWeekday.string(from: due)
            let d = calendar.component(.day, from: due)
            return "\(wkd) \(d)"
        }
    }

    public static func pillSpoken(status: DueStatus, dueOn: String?) -> String {
        switch status {
        case .today:
            return "Due today"
        case .none:
            return "No due date"
        case .overdue:
            return "Overdue, \(spokenDate(dueOn))"
        case .soon:
            return "Due soon, \(spokenDate(dueOn))"
        case .later:
            return "Due later, \(spokenDate(dueOn))"
        }
    }

    public static func decisionSpoken(
        title: String,
        reason: String,
        age: String,
        primary: String,
        secondary: String
    ) -> String {
        "Decision: \(title), \(reason), \(age), \(primary), \(secondary)"
    }

    public static func destructiveHint(module: String, action: String) -> String {
        "Runs \(module).\(action) — changes things and cannot be undone"
    }

    public static let checkboxSpoken = "Mark done"

    /// Filter segment titles; a count shows only when it is above zero.
    public static func filterLabels(todoCount: Int, updateCount: Int) -> [String] {
        ["All", counted("To-dos", todoCount), counted("Updates", updateCount)]
    }

    private static func counted(_ title: String, _ count: Int) -> String {
        count > 0 ? "\(title) \(count)" : title
    }

    public static func age(raisedAtMs: Int64, nowMs: Int64) -> String {
        let raised = Date(timeIntervalSince1970: TimeInterval(raisedAtMs) / 1000)
        let now = Date(timeIntervalSince1970: TimeInterval(nowMs) / 1000)
        let formatter = RelativeDateTimeFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.unitsStyle = .full
        return formatter.localizedString(for: raised, relativeTo: now)
    }

    private static func spokenDate(_ dueOn: String?) -> String {
        guard let dueOn, let due = day.date(from: dueOn) else { return "" }
        return spokenDay.string(from: due)
    }
}
