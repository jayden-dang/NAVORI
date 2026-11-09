import Foundation
import NavoriPanel
import Testing

@Suite
struct NeedsYouTextTests {
    private static let today = "2026-11-08"

    @Test func testPillText() {
        let cases: [(String?, String)] = [
            ("2026-11-08", "Today"),
            ("2026-11-09", "Tomorrow"),
            ("2026-11-10", "Tue"),
            ("2026-11-13", "Fri"),
            ("2026-11-14", "Sat"),
            ("2026-11-15", "Sun 15"),
            ("2026-11-05", "Thu 5"),
            ("2026-11-07", "Sat 7"),
            (nil, "No date"),
        ]
        for (dueOn, expected) in cases {
            #expect(NeedsYouText.pill(dueOn: dueOn, today: Self.today) == expected)
        }
    }

    @Test func filter_labels_show_a_count_only_when_there_is_one() {
        #expect(NeedsYouText.filterLabels(todoCount: 0, updateCount: 0) == ["All", "To-dos", "Updates"])
        #expect(NeedsYouText.filterLabels(todoCount: 3, updateCount: 0) == ["All", "To-dos 3", "Updates"])
        #expect(NeedsYouText.filterLabels(todoCount: 0, updateCount: 12) == ["All", "To-dos", "Updates 12"])
    }

    @Test func testPillSpoken() {
        let cases: [(DueStatus, String?, String)] = [
            (.overdue, "2026-11-05", "Overdue, Thursday 5 November"),
            (.today, "2026-11-08", "Due today"),
            (.soon, "2026-11-08", "Due soon, Sunday 8 November"),
            (.later, "2026-11-13", "Due later, Friday 13 November"),
            (.none, nil, "No due date"),
        ]
        for (status, dueOn, expected) in cases {
            #expect(NeedsYouText.pillSpoken(status: status, dueOn: dueOn) == expected)
        }
    }

    @Test func testDecisionSpoken() {
        #expect(
            NeedsYouText.decisionSpoken(
                title: "Review PR #41 in zroute",
                reason: "Minh requested your review",
                age: "25 minutes ago",
                primary: "Open",
                secondary: "Later"
            ) == "Decision: Review PR #41 in zroute, Minh requested your review, 25 minutes ago, Open, Later"
        )
    }

    @Test func testDestructiveHint() {
        #expect(
            NeedsYouText.destructiveHint(module: "ports", action: "kill")
                == "Runs ports.kill — changes things and cannot be undone"
        )
    }

    @Test func testCheckboxSpokenIsMarkDone() {
        #expect(NeedsYouText.checkboxSpoken == "Mark done")
    }
}
