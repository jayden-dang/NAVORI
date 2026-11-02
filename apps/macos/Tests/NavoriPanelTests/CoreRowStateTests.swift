import NavoriPanel
import Testing

@Suite
struct CoreRowStateTests {
    @Test func each_status_has_its_label_dot_and_submenu() {
        let cases: [(CoreStatus, String, StatusDot?, Bool)] = [
            (.starting, "Starting", nil, false),
            (.running, "Running", .green, true),
            (.needsAttention, "Needs attention", .orange, true),
            (.notRunning, "Not running", .red, false),
        ]
        for (status, label, dot, hasSubmenu) in cases {
            let row = CoreRowState(status: status)
            #expect(row.label == label)
            #expect(row.dot == dot)
            #expect(row.hasSubmenu == hasSubmenu)
        }
    }

    @Test func module_dots_follow_the_module_state() {
        let cases: [(String, StatusDot)] = [
            ("running", .green),
            ("starting", .yellow),
            ("restarting", .yellow),
            ("failed", .red),
            ("stopped", .gray),
        ]
        for (state, expected) in cases {
            #expect(ModuleDot.color(state: state) == expected)
        }
    }
}
