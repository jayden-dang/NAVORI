import AppKit
import NavoriPanel
import SwiftUI

struct DueCalendarView: View {
    let store: CoreStore
    @Environment(CascadeController.self) private var cascade

    fileprivate static let dayFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.timeZone = TimeZone.current
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter
    }()

    private var today: String {
        store.needsYou?.today ?? ""
    }

    private var tomorrow: String {
        Self.nextDay(after: today)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            DueDatePicker(
                selected: store.todoDraftDueOn,
                onPick: { day in
                    store.todoDraftDueOn = day
                    cascade.key(.left)
                }
            )
            .frame(width: 280, height: 190)
            HStack(spacing: 6) {
                CapsuleButton(title: "Today", action: { pick(today) }, fontSize: 11.5, minHeight: 20)
                CapsuleButton(title: "Tomorrow", action: { pick(tomorrow) }, fontSize: 11.5, minHeight: 20)
                CapsuleButton(title: "No date", action: { pick(nil) }, fontSize: 11.5, minHeight: 20)
            }
            .padding(.horizontal, 4)
        }
        .padding(MenuMetrics.pad)
        .frame(width: 300, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
    }

    private func pick(_ day: String?) {
        store.todoDraftDueOn = day
        cascade.key(.left)
    }

    static func nextDay(after day: String) -> String {
        let formatter = dayFormatter
        guard let date = formatter.date(from: day) else { return day }
        let next = Calendar.current.date(byAdding: .day, value: 1, to: date) ?? date
        return formatter.string(from: next)
    }
}

private struct DueDatePicker: NSViewRepresentable {
    var selected: String?
    var onPick: (String) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(onPick: onPick)
    }

    func makeNSView(context: Context) -> NSDatePicker {
        let picker = FirstMouseDatePicker()
        picker.datePickerStyle = .clockAndCalendar
        picker.datePickerElements = .yearMonthDay
        picker.datePickerMode = .single
        picker.isBordered = false
        picker.drawsBackground = false
        picker.target = context.coordinator
        picker.action = #selector(Coordinator.changed(_:))
        if let selected, let date = Coordinator.parse(selected) {
            picker.dateValue = date
        }
        return picker
    }

    func updateNSView(_ picker: NSDatePicker, context: Context) {
        context.coordinator.onPick = onPick
        if let selected, let date = Coordinator.parse(selected),
           Calendar.current.startOfDay(for: picker.dateValue) != Calendar.current.startOfDay(for: date)
        {
            picker.dateValue = date
        }
    }

    @MainActor
    final class Coordinator: NSObject {
        var onPick: (String) -> Void

        init(onPick: @escaping (String) -> Void) {
            self.onPick = onPick
        }

        @objc func changed(_ sender: NSDatePicker) {
            onPick(Self.format(sender.dateValue))
        }

        static func parse(_ day: String) -> Date? {
            DueCalendarView.dayFormatter.date(from: day)
        }

        static func format(_ date: Date) -> String {
            DueCalendarView.dayFormatter.string(from: date)
        }
    }
}

private final class FirstMouseDatePicker: NSDatePicker {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}
