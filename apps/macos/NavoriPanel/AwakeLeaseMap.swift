import Foundation

/// Plain lease fields from a snapshot, with no generated core types.
public enum AwakeLeaseMap {
    /// Level-0 source. A session names the harness; other leases use the published label.
    public static func sourceName(kind: AwakeLeaseInput.Kind, harness: String?, label: String?) -> String? {
        let name = kind == .session ? harness.flatMap(HarnessCopy.display) : label
        guard let name, name.isEmpty == false else { return nil }
        return name
    }

    public static func lease(
        kind: AwakeLeaseInput.Kind,
        harness: String?,
        label: String?,
        pid: Int32?,
        stopped: Bool,
        endsAtMs: Int64?,
        title: String? = nil,
        project: String? = nil,
        timeZone: TimeZone = .current
    ) -> AwakeLeaseInput {
        let until = kind == .duration ? endsAtMs.map { clockText($0, timeZone: timeZone) } : nil
        return AwakeLeaseInput(
            kind: kind,
            harness: nonempty(harness).flatMap(HarnessCopy.display),
            label: nonempty(label),
            pid: pid,
            stopped: stopped,
            until: until,
            title: nonempty(title),
            project: nonempty(project)
        )
    }

    /// `HH:mm` on the POSIX gregorian clock. The zone defaults to the current zone, matching the menu.
    public static func clockText(_ ms: Int64, timeZone: TimeZone = .current) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.timeZone = timeZone
        formatter.dateFormat = "HH:mm"
        return formatter.string(from: Date(timeIntervalSince1970: TimeInterval(ms) / 1000))
    }

    private static func nonempty(_ value: String?) -> String? {
        guard let value, value.isEmpty == false else { return nil }
        return value
    }
}
