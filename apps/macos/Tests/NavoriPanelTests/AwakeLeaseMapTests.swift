import Foundation
import NavoriPanel
import Testing

@Suite
struct AwakeLeaseMapTests {
    @Test func an_unknown_lease_kind_is_dropped() {
        let lease = AwakeLeaseMap.lease(
            kind: "not-a-lease",
            harness: "Claude Code",
            label: "1 hour",
            pid: 12,
            stopped: false,
            endsAtMs: 1_700_000_000_000
        )
        #expect(lease == nil)
    }

    @Test func a_session_row_uses_the_harness() {
        let source = AwakeLeaseMap.sourceName(
            kind: "session",
            harness: "Claude Code",
            label: "NAVORI/main"
        )
        let status = AwakeLevel0.row(
            problem: nil,
            systemHeld: true,
            hasLease: true,
            sources: [source].compactMap { $0 }
        )
        #expect(AwakeRowState(status: status).label == "On · Claude Code")
    }

    @Test func a_session_title_reaches_the_row_input() {
        let lease = AwakeLeaseMap.lease(
            kind: "session", harness: "Claude Code", label: "klynt/dev", pid: 1, stopped: false, endsAtMs: nil,
            title: "Road-128"
        )
        #expect(lease?.title == "Road-128")
    }

    @Test func a_known_epoch_formats_as_hour_and_minute() {
        let zone = TimeZone(secondsFromGMT: 0)!
        let ms: Int64 = 1_700_000_000_000
        let lease = AwakeLeaseMap.lease(
            kind: "duration",
            harness: nil,
            label: "1 hour",
            pid: nil,
            stopped: false,
            endsAtMs: ms,
            timeZone: zone
        )
        let state = AwakeSubmenuState(
            status: .on(sources: ["1 hour"]),
            leases: [lease].compactMap { $0 },
            displayHeld: false,
            tracking: true
        )
        #expect(lease?.until == "22:13")
        #expect(state.keepValue == "until 22:13")
        #expect(AwakeLeaseMap.clockText(ms, timeZone: zone) == "22:13")
    }
}
