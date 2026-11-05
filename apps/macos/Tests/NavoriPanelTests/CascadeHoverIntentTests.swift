import CoreGraphics
import NavoriPanel
import Testing

/// When a hovered row opens its Submenu: counted from the first entry into the row, at once when switching
/// between siblings, and only after the pointer rests while it travels the safe diagonal.
@Suite
struct CascadeHoverIntentTests {
    private static let tree: [CascadeNode] = [
        CascadeNode(id: "awake", isActionable: true, children: [CascadeNode(id: "awake.keep", isActionable: true)]),
        CascadeNode(id: "core", isActionable: true, children: [CascadeNode(id: "core.modules", isActionable: true)]),
        CascadeNode(id: "settings", isActionable: true),
        CascadeNode(id: "quit", isActionable: true),
    ]
    private static let submenu = CGRect(x: 400, y: 100, width: 200, height: 200)
    private static let intent = CascadeEffect.armHoverTimer(after: CascadeState.intentDelay)
    private static let rest = CascadeEffect.armHoverTimer(after: CascadeState.restDelay)

    private static func point(_ x: CGFloat, _ y: CGFloat = 200) -> CGPoint { CGPoint(x: x, y: y) }

    @Test func the_intent_delay_is_fifty_milliseconds_and_shorter_than_the_rest_delay() {
        #expect(CascadeState.intentDelay == 0.05)
        #expect(CascadeState.restDelay == 0.15)
    }

    @Test func entering_a_row_with_a_submenu_arms_the_intent_delay_once() {
        var state = CascadeState(root: Self.tree)
        let entered = state.handle(.pointerOn(row: "core", at: Self.point(100), submenu: nil))
        #expect(entered == [Self.intent])
        for x in stride(from: CGFloat(103), through: 160, by: 3) {
            let moved = state.handle(.pointerOn(row: "core", at: Self.point(x, 200 + x / 10), submenu: nil))
            #expect(moved.isEmpty, "movement at x=\(x) must not restart the timer")
        }
        #expect(state.handle(.hoverTimerFired) == [.open(row: "core")])
        #expect(state.openPath == ["core"])
    }

    @Test func movement_inside_an_open_row_arms_nothing() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.pointerOn(row: "core", at: Self.point(100), submenu: nil))
        _ = state.handle(.hoverTimerFired)
        #expect(state.openPath == ["core"])
        let moved = state.handle(.pointerOn(row: "core", at: Self.point(110), submenu: nil))
        #expect(moved.isEmpty)
        #expect(state.handle(.pointerLeft(row: "core")).isEmpty)
    }

    @Test func moving_to_a_row_without_children_cancels_the_pending_open() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.pointerOn(row: "core", at: Self.point(100), submenu: nil))
        let moved = state.handle(.pointerOn(row: "settings", at: Self.point(100, 174), submenu: nil))
        #expect(moved == [.cancelHoverTimer])
        #expect(state.selection == "settings")
        #expect(state.handle(.hoverTimerFired).isEmpty)
        #expect(state.openPath == [])
    }

    @Test func sweeping_down_across_rows_never_opens_a_submenu() {
        var state = CascadeState(root: Self.tree)
        var opened: [CascadeEffect] = []
        for (index, row) in ["awake", "core", "settings", "quit"].enumerated() {
            opened += state.handle(.pointerOn(row: row, at: Self.point(100, 300 - CGFloat(index) * 26), submenu: nil))
        }
        #expect(opened.contains { if case .open = $0 { true } else { false } } == false)
        #expect(state.handle(.hoverTimerFired).isEmpty)
        #expect(state.openPath == [])
    }

    @Test func moving_to_another_row_with_children_restarts_for_that_row_only() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.pointerOn(row: "awake", at: Self.point(100, 300), submenu: nil))
        let moved = state.handle(.pointerOn(row: "core", at: Self.point(100, 274), submenu: nil))
        #expect(moved == [Self.intent])
        #expect(state.handle(.hoverTimerFired) == [.open(row: "core")])
    }

    @Test func a_row_without_children_arms_no_timer() {
        var state = CascadeState(root: Self.tree)
        let entered = state.handle(.pointerOn(row: "settings", at: Self.point(100), submenu: nil))
        #expect(entered.isEmpty)
        #expect(state.selection == "settings")
        #expect(state.handle(.pointerOn(row: "settings", at: Self.point(110), submenu: nil)).isEmpty)
    }

    @Test func a_row_without_children_still_closes_the_deeper_submenus() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.pointerOn(row: "core", at: Self.point(100), submenu: nil))
        _ = state.handle(.click(row: "core"))
        let left = state.handle(.pointerOn(row: "settings", at: Self.point(50), submenu: Self.submenu))
        #expect(left == [.close(depth: 1)])
        #expect(state.openPath == [])
    }

    @Test func switching_to_a_sibling_with_children_closes_and_opens_in_one_step() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.pointerOn(row: "awake", at: Self.point(100), submenu: nil))
        _ = state.handle(.click(row: "awake"))
        let switched = state.handle(.pointerOn(row: "core", at: Self.point(50), submenu: Self.submenu))
        #expect(switched == [.close(depth: 1), .open(row: "core")])
        #expect(state.openPath == ["core"])
        #expect(state.selection == "core")
        #expect(state.handle(.hoverTimerFired).isEmpty)
    }

    @Test func switching_cancels_a_timer_left_pending_by_the_diagonal() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.pointerOn(row: "awake", at: Self.point(100), submenu: nil))
        _ = state.handle(.click(row: "awake"))
        #expect(state.handle(.pointerOn(row: "core", at: Self.point(250), submenu: Self.submenu)) == [Self.rest])
        let switched = state.handle(.pointerOn(row: "core", at: Self.point(50), submenu: Self.submenu))
        #expect(switched == [.cancelHoverTimer, .close(depth: 1), .open(row: "core")])
        #expect(state.handle(.hoverTimerFired).isEmpty)
    }

    @Test func the_diagonal_rearms_the_rest_delay_on_each_move_and_opens_a_crossed_row_only_at_rest() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.pointerOn(row: "awake", at: Self.point(100), submenu: nil))
        _ = state.handle(.click(row: "awake"))
        for x in stride(from: CGFloat(250), through: 262, by: 3) {
            let crossed = state.handle(.pointerOn(row: "core", at: Self.point(x), submenu: Self.submenu))
            #expect(crossed == [Self.rest], "x=\(x)")
            #expect(state.openPath == ["awake"], "a crossed row must not open mid-travel")
            #expect(state.selection == "awake")
        }
        #expect(state.handle(.hoverTimerFired) == [.close(depth: 1), .open(row: "core")])
        #expect(state.openPath == ["core"])
    }

    @Test func a_row_with_children_at_the_deepest_panel_opens_after_the_intent_delay() {
        var state = CascadeState(root: [
            CascadeNode(id: "a", isActionable: true, children: [
                CascadeNode(id: "a.b", isActionable: true, children: [CascadeNode(id: "a.b.c", isActionable: true)]),
            ]),
        ], maxDepth: 3)
        _ = state.handle(.click(row: "a"))
        let entered = state.handle(.pointerOn(row: "a.b", at: Self.point(300), submenu: Self.submenu))
        #expect(entered == [Self.intent])
        #expect(state.handle(.hoverTimerFired) == [.open(row: "a.b")])
    }

    @Test func a_click_and_a_key_still_cancel_the_pending_intent() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.pointerOn(row: "core", at: Self.point(100), submenu: nil))
        #expect(state.handle(.key(.down)) == [.cancelHoverTimer])
        #expect(state.handle(.hoverTimerFired).isEmpty)
    }

    // MARK: what the submenu_intent_open_ms gate (80 ms) is made of

    /// Replays pointer moves through the state and returns when the hover timer would fire, in seconds after the
    /// first entry, following arm and cancel effects the way the controller does.
    private static func timerFiresAfter(moves: [(row: String, x: CGFloat, t: Double)], submenu: CGRect?) -> Double? {
        var state = CascadeState(root: tree)
        var deadline: Double?
        for move in moves {
            let effects = state.handle(.pointerOn(row: move.row, at: point(move.x), submenu: submenu))
            for effect in effects {
                switch effect {
                case let .armHoverTimer(delay): deadline = move.t + delay
                case .cancelHoverTimer: deadline = nil
                default: break
                }
            }
        }
        return deadline
    }

    @Test func a_pointer_that_keeps_moving_inside_the_row_still_opens_fifty_ms_after_entry() {
        let moves = (0..<12).map { (row: "core", x: CGFloat(100 + $0 * 3), t: Double($0) * 0.016) }
        #expect(Self.timerFiresAfter(moves: moves, submenu: nil) == CascadeState.intentDelay)
    }

    @Test func a_timer_that_rearms_on_every_move_at_150_ms_would_miss_the_80_ms_gate() {
        // The old bug re-armed 150 ms after the LAST move. The safe-diagonal branch still works that way, so it
        // is the live model of it: 12 moves 16 ms apart end at 176 ms and the timer fires at 326 ms. The gate
        // (intent delay + the 16 ms open budget, measured against 80 ms) separates the two shapes.
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.pointerOn(row: "awake", at: Self.point(100), submenu: nil))
        _ = state.handle(.click(row: "awake"))
        var deadline = 0.0
        for index in 0..<12 {
            let effects = state.handle(.pointerOn(row: "core", at: Self.point(250 + CGFloat(index) * 3), submenu: Self.submenu))
            #expect(effects == [Self.rest])
            deadline = Double(index) * 0.016 + CascadeState.restDelay
        }
        let gate = 0.080
        #expect(deadline > gate, "re-arming at 150 ms ends at \(deadline) s")
        #expect(CascadeState.intentDelay + 0.016 < gate, "the 50 ms intent path plus the open budget fits")
    }
}
