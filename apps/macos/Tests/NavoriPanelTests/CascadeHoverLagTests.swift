import CoreGraphics
import NavoriPanel
import Testing

/// Pointer paths at the geometry of the real menu: level 0 on the right, each level opening left.
@Suite
struct CascadeHoverLagTests {
    private static let keepIds = ["indefinitely", "300", "600", "900", "1800", "3600", "7200", "18000"]
        .map { "awake.keep.\($0)" }
    private static let tree = [
        CascadeNode(id: "awake", isActionable: true, children: [
            CascadeNode(
                id: "awake.keep",
                isActionable: true,
                children: keepIds.map { CascadeNode(id: $0, isActionable: true) }
            ),
            CascadeNode(id: "awake.tracking", isActionable: true),
        ]),
        CascadeNode(id: "core", isActionable: true, children: [CascadeNode(id: "core.modules")]),
        CascadeNode(id: "settings", isActionable: true),
        CascadeNode(id: "about", isActionable: true),
    ]

    private static let level1 = CGRect(x: 736, y: 650, width: 360, height: 120)
    private static let level2 = CGRect(x: 552, y: 552, width: 180, height: 218)

    /// y of a row's centre in a panel, rows 26 pt tall from the top, 5 pt padding.
    private static func y(row: Int, in panel: CGRect) -> CGFloat {
        panel.maxY - 5 - 13 - CGFloat(row) * 26
    }

    /// Walks 3 pt at a time, the size of a real mouse event, and counts steps whose row is not selected.
    private static func sweep(
        panel: CGRect,
        rows: [String],
        open: [String],
        start: CGPoint,
        degrees: CGFloat
    ) -> (steps: Int, missed: Int, longest: Int) {
        var state = CascadeState(root: tree)
        for id in open { _ = state.handle(.click(row: id)) }
        let radians = degrees * .pi / 180
        var point = start
        var steps = 0, missed = 0, run = 0, longest = 0
        while true {
            let index = Int(((panel.maxY - 5) - point.y) / 26)
            guard panel.insetBy(dx: 5, dy: 5).contains(point), rows.indices.contains(index) else { break }
            _ = state.handle(.pointerOn(row: rows[index], at: point, submenu: panel))
            steps += 1
            if state.selection == rows[index] {
                run = 0
            } else {
                missed += 1
                run += 1
                longest = max(longest, run)
            }
            point.x -= 3 * cos(radians)
            point.y -= 3 * sin(radians)
        }
        return (steps, missed, longest)
    }

    @Test func rows_of_the_deepest_panel_follow_the_pointer_at_every_angle() {
        for degrees in stride(from: CGFloat(0), through: 80, by: 10) {
            let result = Self.sweep(
                panel: Self.level2,
                rows: Self.keepIds,
                open: ["awake", "awake.keep"],
                start: CGPoint(x: Self.level2.maxX - 6, y: Self.y(row: 0, in: Self.level2)),
                degrees: degrees
            )
            #expect(result.missed == 0, "angle \(degrees): \(result)")
        }
    }

    @Test func rows_of_the_first_submenu_follow_the_pointer_at_every_angle() {
        let rows = ["awake.keep", "awake.tracking"]
        for degrees in stride(from: CGFloat(0), through: 40, by: 10) {
            let result = Self.sweep(
                panel: Self.level1,
                rows: rows,
                open: ["awake"],
                start: CGPoint(x: Self.level1.maxX - 6, y: Self.y(row: 0, in: Self.level1)),
                degrees: degrees
            )
            #expect(result.missed == 0, "angle \(degrees): \(result)")
        }
    }

    /// Move left along a parent row by `left`, then straight down onto the next row, near its far side.
    private static func parentWalk(left: CGFloat) -> (down: Int, missed: Int, landed: Bool) {
        var state = CascadeState(root: tree)
        _ = state.handle(.click(row: "awake"))
        _ = state.handle(.click(row: "awake.keep"))
        var point = CGPoint(x: level1.maxX - 6, y: y(row: 0, in: level1))
        _ = state.handle(.pointerOn(row: "awake.keep", at: point, submenu: level2))
        var moved: CGFloat = 0
        while moved < left {
            point.x -= 3
            moved += 3
            _ = state.handle(.pointerOn(row: "awake.keep", at: point, submenu: level2))
        }
        var down = 0, missed = 0
        let boundary = y(row: 0, in: level1) - 13
        while point.y > boundary - 24 {
            point.y -= 3
            let row = point.y > boundary ? "awake.keep" : "awake.tracking"
            _ = state.handle(.pointerOn(row: row, at: point, submenu: level2))
            if row == "awake.tracking" {
                down += 1
                if state.selection != row { missed += 1 }
            }
        }
        return (down, missed, state.selection == "awake.tracking")
    }

    @Test func a_parent_row_follows_a_pointer_that_turns_away_from_the_submenu() {
        for left in [CGFloat(0), 30, 90, 180, 300] {
            let result = Self.parentWalk(left: left)
            #expect(result.landed, "left \(left): \(result)")
            #expect(result.missed <= 5, "left \(left): \(result)")
        }
    }

    /// Real mouse events toward the Submenu are uneven: some have no horizontal part at all.
    @Test func an_uneven_trip_to_the_submenu_keeps_it_open() {
        for target in 0..<8 {
            for pattern in [[true, false, false], [true, true, false, false, false], [false, true, true, false]] {
                var state = CascadeState(root: Self.tree)
                _ = state.handle(.click(row: "awake"))
                _ = state.handle(.click(row: "awake.keep"))
                let start = CGPoint(x: Self.level1.maxX - 40, y: Self.y(row: 0, in: Self.level1))
                var point = start
                _ = state.handle(.pointerOn(row: "awake.keep", at: point, submenu: Self.level2))
                let goal = CGPoint(x: Self.level2.maxX + 2, y: Self.y(row: target, in: Self.level2))
                let slope = (goal.y - start.y) / (goal.x - start.x)
                var step = 0
                while point.x > goal.x {
                    let wantY = start.y + slope * (point.x - start.x)
                    if pattern[step % pattern.count], abs(wantY - point.y) >= 1 {
                        point.y += wantY < point.y ? -1 : 1
                    } else {
                        point.x -= 1
                    }
                    step += 1
                    let row = point.y > start.y - 13 ? "awake.keep" : "awake.tracking"
                    _ = state.handle(.pointerOn(row: row, at: point, submenu: Self.level2))
                    #expect(state.openPath == ["awake", "awake.keep"], "target \(target) at \(point)")
                    #expect(state.selection == "awake.keep", "target \(target) at \(point)")
                }
            }
        }
    }

    @Test func a_trip_from_level_zero_to_the_first_submenu_keeps_it_open() {
        let levelOne = CGRect(x: 736, y: 400, width: 360, height: 370)
        for targetY in stride(from: levelOne.minY + 8, through: levelOne.maxY - 8, by: 40) {
            var state = CascadeState(root: Self.tree)
            _ = state.handle(.click(row: "awake"))
            let start = CGPoint(x: 1300, y: 757)
            var point = start
            _ = state.handle(.pointerOn(row: "awake", at: point, submenu: levelOne))
            let slope = (targetY - start.y) / (levelOne.maxX + 2 - start.x)
            while point.x > levelOne.maxX + 2 {
                point.x -= 3
                point.y = start.y + slope * (point.x - start.x)
                let row = point.y > 744 ? "awake" : point.y > 718 ? "core" : "settings"
                _ = state.handle(.pointerOn(row: row, at: point, submenu: levelOne))
                #expect(state.openPath == ["awake"], "target \(targetY) at \(point)")
                #expect(state.selection == "awake", "target \(targetY) at \(point)")
            }
        }
    }
}
