import NavoriPanel
import Testing

@Suite
struct AwakeRowStateTests {
    @Test func each_status_has_its_label_dot_and_submenu() {
        let cases: [(AwakeStatus, String, StatusDot?, Bool)] = [
            (.off, "Off", nil, true),
            (.on(sources: ["Claude Code", "1 hour"]), "On · Claude Code, 1 hour", .green, true),
            (.notHeld, "Not held", .orange, true),
            (.unavailable, "Unavailable", nil, false),
        ]
        for (status, label, dot, hasSubmenu) in cases {
            let row = AwakeRowState(status: status)
            #expect(row.label == label)
            #expect(row.dot == dot)
            #expect(row.hasSubmenu == hasSubmenu)
        }
    }
}
