import Foundation
import NavoriPanel
import Testing

@Suite
struct LastSleepTextTests {
    private static let calendar: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Asia/Ho_Chi_Minh")!
        return calendar
    }()

    private static func ms(_ year: Int, _ month: Int, _ day: Int, _ hour: Int, _ minute: Int) -> Int64 {
        let parts = DateComponents(year: year, month: month, day: day, hour: hour, minute: minute)
        return Int64(calendar.date(from: parts)!.timeIntervalSince1970 * 1000)
    }

    @Test func the_last_sleep_reads_by_calendar_day() {
        let now = Self.calendar.date(from: DateComponents(year: 2026, month: 11, day: 6, hour: 10))!
        let rows: [(Int64, String, String?)] = [
            (Self.ms(2026, 11, 6, 9, 20), "09:20", "today at 09:20"),
            (Self.ms(2026, 11, 5, 12, 3), "Yesterday 12:03", "yesterday at 12:03"),
            (Self.ms(2026, 11, 4, 11, 43), "Wed 11:43", "Wednesday at 11:43"),
            (Self.ms(2026, 10, 30, 8, 0), "30 Oct", nil),
            (Self.ms(2025, 10, 30, 8, 0), "30 Oct 2025", nil),
        ]
        for (ms, value, spoken) in rows {
            let text = lastSleepText(ms: ms, now: now, calendar: Self.calendar)
            #expect(text.value == value)
            if let spoken { #expect(text.spoken == spoken) }
            #expect(text.spoken.isEmpty == false)
        }
    }
}
