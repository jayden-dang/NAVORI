import CoreGraphics
import NavoriPanel
import Testing

@Suite
struct CascadeStateTests {
    @Test func resting_on_a_row_with_a_submenu_opens_it_when_the_timer_fires() {
        var state = CascadeState(root: Self.tree)
        let armed = state.handle(
            .pointerOn(row: "core", at: CGPoint(x: 100, y: 200), submenu: nil)
        )
        #expect(armed == [.armHoverTimer(after: CascadeState.intentDelay)])
        #expect(state.openPath == [])
        #expect(state.selection == "core")
        let opened = state.handle(.hoverTimerFired)
        #expect(opened == [.open(row: "core")])
        #expect(state.openPath == ["core"])
        #expect(state.selection == "core")
    }

    @Test func a_click_on_a_row_with_a_submenu_opens_it_at_once() {
        var state = CascadeState(root: Self.tree)
        let effects = state.handle(.click(row: "core"))
        #expect(effects == [.open(row: "core")])
        #expect(state.openPath == ["core"])
        #expect(state.selection == "core")
    }

    @Test func a_click_on_an_action_row_activates_it() {
        var state = CascadeState(root: Self.tree)
        let effects = state.handle(.click(row: "quit"))
        #expect(effects == [.activate(row: "quit")])
        #expect(state.openPath == [])
    }

    @Test func moving_inside_the_diagonal_keeps_the_submenu_and_the_selection() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(
            .pointerOn(row: "core", at: CGPoint(x: 100, y: 200), submenu: nil)
        )
        _ = state.handle(.click(row: "core"))
        let effects = state.handle(
            .pointerOn(
                row: "settings",
                at: CGPoint(x: 250, y: 200),
                submenu: Self.submenu
            )
        )
        #expect(effects == [.armHoverTimer(after: CascadeState.restDelay)])
        #expect(state.selection == "core")
        #expect(state.openPath == ["core"])
    }

    @Test func resting_on_another_row_inside_the_diagonal_switches_when_the_timer_fires() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(
            .pointerOn(row: "core", at: CGPoint(x: 100, y: 200), submenu: nil)
        )
        _ = state.handle(.click(row: "core"))
        _ = state.handle(
            .pointerOn(
                row: "settings",
                at: CGPoint(x: 250, y: 200),
                submenu: Self.submenu
            )
        )
        #expect(state.selection == "core")
        #expect(state.openPath == ["core"])
        let effects = state.handle(.hoverTimerFired)
        #expect(effects == [.close(depth: 1)])
        #expect(state.selection == "settings")
        #expect(state.openPath == [])
    }

    @Test func leaving_the_diagonal_selects_the_row_and_closes_the_submenu_it_does_not_own() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(
            .pointerOn(row: "core", at: CGPoint(x: 100, y: 200), submenu: nil)
        )
        _ = state.handle(.click(row: "core"))
        let effects = state.handle(
            .pointerOn(
                row: "settings",
                at: CGPoint(x: 50, y: 200),
                submenu: Self.submenu
            )
        )
        #expect(effects == [.close(depth: 1)])
        #expect(state.selection == "settings")
        #expect(state.openPath == [])
    }

    @Test func every_new_pointer_position_on_the_diagonal_rearms_the_rest_timer() {
        var state = CascadeState(root: Self.tree)
        let first = state.handle(
            .pointerOn(row: "core", at: CGPoint(x: 100, y: 200), submenu: nil)
        )
        #expect(first == [.armHoverTimer(after: CascadeState.intentDelay)])
        #expect(state.selection == "core")
        #expect(state.openPath == [])
        _ = state.handle(.click(row: "core"))
        #expect(state.selection == "core")
        #expect(state.openPath == ["core"])
        let second = state.handle(
            .pointerOn(
                row: "settings",
                at: CGPoint(x: 250, y: 200),
                submenu: Self.submenu
            )
        )
        #expect(second == [.armHoverTimer(after: CascadeState.restDelay)])
        #expect(state.selection == "core")
        #expect(state.openPath == ["core"])
        let third = state.handle(
            .pointerOn(
                row: "quit",
                at: CGPoint(x: 260, y: 205),
                submenu: Self.submenu
            )
        )
        #expect(third == [.armHoverTimer(after: CascadeState.restDelay)])
        #expect(state.selection == "core")
        #expect(state.openPath == ["core"])
    }

    @Test func the_same_pointer_position_twice_changes_nothing() {
        var state = CascadeState(root: Self.tree)
        let point = CGPoint(x: 100, y: 200)
        let first = state.handle(.pointerOn(row: "core", at: point, submenu: nil))
        #expect(first == [.armHoverTimer(after: CascadeState.intentDelay)])
        #expect(state.selection == "core")
        #expect(state.openPath == [])
        let second = state.handle(.pointerOn(row: "core", at: point, submenu: nil))
        #expect(second.isEmpty)
        #expect(state.selection == "core")
        #expect(state.openPath == [])
        let otherRow = state.handle(.pointerOn(row: "settings", at: point, submenu: nil))
        #expect(otherRow.isEmpty)
        #expect(state.selection == "core")
        #expect(state.openPath == [])
        let timed = state.handle(.hoverTimerFired)
        #expect(timed == [.open(row: "core")])
        #expect(state.selection == "core")
        #expect(state.openPath == ["core"])
    }

    @Test func leaving_a_row_clears_its_selection_unless_it_owns_an_open_submenu() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(
            .pointerOn(row: "quit", at: CGPoint(x: 10, y: 10), submenu: nil)
        )
        #expect(state.selection == "quit")
        let leftQuit = state.handle(.pointerLeft(row: "quit"))
        #expect(leftQuit.isEmpty)
        #expect(state.selection == nil)

        _ = state.handle(.click(row: "core"))
        #expect(state.selection == "core")
        #expect(state.openPath == ["core"])
        let leftCore = state.handle(.pointerLeft(row: "core"))
        #expect(leftCore.isEmpty)
        #expect(state.selection == "core")
        #expect(state.openPath == ["core"])
    }

    @Test func leaving_a_different_row_keeps_the_pending_hover() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(
            .pointerOn(row: "settings", at: CGPoint(x: 10, y: 10), submenu: nil)
        )
        let armed = state.handle(
            .pointerOn(row: "core", at: CGPoint(x: 100, y: 200), submenu: nil)
        )
        #expect(armed == [.armHoverTimer(after: CascadeState.intentDelay)])
        #expect(state.selection == "core")
        #expect(state.openPath == [])
        let left = state.handle(.pointerLeft(row: "settings"))
        #expect(left.isEmpty)
        #expect(state.selection == "core")
        #expect(state.openPath == [])
        let opened = state.handle(.hoverTimerFired)
        #expect(opened == [.open(row: "core")])
        #expect(state.openPath == ["core"])
        #expect(state.selection == "core")
    }

    @Test func keep_awake_opens_as_the_third_level_and_its_rows_activate() {
        let tree = [
            CascadeNode(id: "awake", isActionable: true, children: [
                CascadeNode(id: "awake.keep", isActionable: true, children: [
                    CascadeNode(id: "awake.keep.300", isActionable: true),
                ]),
                CascadeNode(id: "awake.tracking", isActionable: true),
            ]),
        ]
        var state = CascadeState(root: tree)
        #expect(state.handle(.click(row: "awake")).contains(.open(row: "awake")))
        #expect(state.handle(.click(row: "awake.keep")).contains(.open(row: "awake.keep")))
        #expect(state.openPath == ["awake", "awake.keep"])
        #expect(state.handle(.click(row: "awake.keep.300")).contains(.activate(row: "awake.keep.300")))
        #expect(state.openPath == ["awake", "awake.keep"])
    }

    @Test func a_row_at_the_maximum_depth_never_opens() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.click(row: "core"))
        _ = state.handle(.click(row: "core.a"))
        #expect(state.openPath == ["core", "core.a"])
        let clicked = state.handle(.click(row: "core.a.x"))
        #expect(clicked.isEmpty)
        #expect(state.selection == "core.a.x")
        #expect(state.openPath == ["core", "core.a"])
        let armed = state.handle(
            .pointerOn(row: "core.a.x", at: CGPoint(x: 20, y: 20), submenu: nil)
        )
        #expect(armed.isEmpty)
        #expect(state.selection == "core.a.x")
        let timed = state.handle(.hoverTimerFired)
        #expect(timed.isEmpty)
        #expect(state.selection == "core.a.x")
        #expect(state.openPath == ["core", "core.a"])
    }

    @Test func a_row_with_no_action_is_never_selected() {
        let tree = Self.tree + [CascadeNode(id: "heading")]
        var state = CascadeState(root: tree)
        let pointed = state.handle(
            .pointerOn(row: "heading", at: CGPoint(x: 8, y: 8), submenu: nil)
        )
        #expect(pointed.isEmpty)
        #expect(state.selection == nil)
        #expect(state.openPath == [])
        let timed = state.handle(.hoverTimerFired)
        #expect(timed.isEmpty)
        #expect(state.selection == nil)
        let clicked = state.handle(.click(row: "heading"))
        #expect(clicked.isEmpty)
        #expect(state.selection == nil)
        #expect(state.openPath == [])
    }

    @Test func a_click_cancels_the_pending_hover() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(
            .pointerOn(row: "core", at: CGPoint(x: 10, y: 10), submenu: nil)
        )
        #expect(state.selection == "core")
        let clicked = state.handle(.click(row: "core"))
        #expect(clicked == [.cancelHoverTimer, .open(row: "core")])
        #expect(state.selection == "core")
        #expect(state.openPath == ["core"])
        let timed = state.handle(.hoverTimerFired)
        #expect(timed.isEmpty)
        #expect(state.selection == "core")
        #expect(state.openPath == ["core"])
    }

    @Test func replacing_the_tree_can_add_children() {
        var state = CascadeState(root: Self.tree)
        var grown = Self.tree
        grown[1] = CascadeNode(
            id: "settings",
            isActionable: true,
            children: [CascadeNode(id: "settings.theme", isActionable: true)]
        )
        let replaced = state.handle(.replaceTree(grown))
        #expect(replaced.isEmpty)
        #expect(state.openPath == [])
        let opened = state.handle(.click(row: "settings"))
        #expect(opened == [.open(row: "settings")])
        #expect(state.openPath == ["settings"])
    }

    @Test func replacing_the_tree_closes_a_submenu_whose_children_are_gone() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.click(row: "core"))
        #expect(state.openPath == ["core"])
        let stripped: [CascadeNode] = [
            CascadeNode(id: "core"),
            CascadeNode(id: "settings", isActionable: true),
            CascadeNode(id: "quit", isActionable: true),
        ]
        let effects = state.handle(.replaceTree(stripped))
        #expect(effects == [.close(depth: 1)])
        #expect(state.openPath == [])
    }

    @Test func a_row_that_vanishes_while_focused_moves_focus_to_its_parent() {
        var state = CascadeState(root: Self.projectTree(["a", "b"]))
        _ = state.handle(.click(row: "running"))
        _ = state.handle(.click(row: "running.project.b"))
        #expect(state.selection == "running.project.b")
        let effects = state.handle(.replaceTree(Self.projectTree(["a"])))
        #expect(effects.isEmpty)
        #expect(state.selection == "running")
        #expect(state.openPath == ["running"])
    }

    @Test func a_row_that_vanishes_with_its_parent_leaves_no_focus() {
        var state = CascadeState(root: Self.projectTree(["a"]))
        _ = state.handle(.click(row: "running"))
        _ = state.handle(.click(row: "running.project.a"))
        let gone: [CascadeNode] = [CascadeNode(id: "settings", isActionable: true)]
        _ = state.handle(.replaceTree(gone))
        #expect(state.selection == nil)
        #expect(state.openPath == [])
    }

    @Test func a_tree_with_project_children_opens_three_levels_deep() {
        let tree = [
            CascadeNode(id: "running", isActionable: true, children: [
                CascadeNode(id: "running.project.a", isActionable: true, children: [
                    CascadeNode(id: "running.project.a.port", isActionable: true),
                ]),
            ]),
        ]
        var state = CascadeState(root: tree)
        #expect(state.handle(.click(row: "running")).contains(.open(row: "running")))
        #expect(state.handle(.click(row: "running.project.a")).contains(.open(row: "running.project.a")))
        #expect(state.openPath == ["running", "running.project.a"])
        #expect(state.handle(.click(row: "running.project.a.port")).contains(.activate(row: "running.project.a.port")))
        #expect(state.openPath == ["running", "running.project.a"])
    }

    @Test func down_and_up_wrap_through_rows_with_an_action() {
        var state = CascadeState(root: Self.tree)
        #expect(state.handle(.key(.down)).isEmpty)
        #expect(state.selection == "core")
        #expect(state.openPath == [])
        #expect(state.handle(.key(.down)).isEmpty)
        #expect(state.selection == "settings")
        #expect(state.handle(.key(.down)).isEmpty)
        #expect(state.selection == "quit")
        #expect(state.handle(.key(.down)).isEmpty)
        #expect(state.selection == "core")
        #expect(state.handle(.key(.up)).isEmpty)
        #expect(state.selection == "quit")
        #expect(state.handle(.key(.up)).isEmpty)
        #expect(state.selection == "settings")
        #expect(state.handle(.key(.up)).isEmpty)
        #expect(state.selection == "core")
        #expect(state.handle(.key(.up)).isEmpty)
        #expect(state.selection == "quit")

        var nested = CascadeState(
            root: [
                CascadeNode(
                    id: "core",
                    children: [
                        CascadeNode(id: "core.a", isActionable: true),
                        CascadeNode(id: "core.b", isActionable: true),
                    ]
                ),
                CascadeNode(id: "settings", isActionable: true),
            ]
        )
        _ = nested.handle(.click(row: "core"))
        #expect(nested.selection == "core")
        #expect(nested.openPath == ["core"])
        #expect(nested.handle(.key(.down)).isEmpty)
        #expect(nested.selection == "core.a")
        #expect(nested.openPath == ["core"])
        #expect(nested.handle(.key(.down)).isEmpty)
        #expect(nested.selection == "core.b")
        #expect(nested.handle(.key(.down)).isEmpty)
        #expect(nested.selection == "core.a")
        #expect(nested.handle(.key(.up)).isEmpty)
        #expect(nested.selection == "core.b")
    }

    @Test func down_skips_rows_with_no_action() {
        let tree: [CascadeNode] = [
            CascadeNode(
                id: "core",
                children: [CascadeNode(id: "core.a", isActionable: true)]
            ),
            CascadeNode(id: "heading"),
            CascadeNode(id: "settings", isActionable: true),
            CascadeNode(id: "quit", isActionable: true),
        ]
        var state = CascadeState(root: tree)
        _ = state.handle(.key(.down))
        #expect(state.selection == "core")
        _ = state.handle(.key(.down))
        #expect(state.selection == "settings")
        _ = state.handle(.key(.down))
        #expect(state.selection == "quit")
        _ = state.handle(.key(.down))
        #expect(state.selection == "core")
        _ = state.handle(.key(.up))
        #expect(state.selection == "quit")
        _ = state.handle(.key(.up))
        #expect(state.selection == "settings")
        _ = state.handle(.key(.up))
        #expect(state.selection == "core")
    }

    @Test func right_opens_the_submenu_and_moves_in() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.key(.down))
        #expect(state.selection == "core")
        let opened = state.handle(.key(.right))
        #expect(opened == [.open(row: "core")])
        #expect(state.openPath == ["core"])
        #expect(state.selection == "core.a")

        var alreadyOpen = CascadeState(root: Self.tree)
        _ = alreadyOpen.handle(.click(row: "core"))
        #expect(alreadyOpen.selection == "core")
        #expect(alreadyOpen.openPath == ["core"])
        let moved = alreadyOpen.handle(.key(.right))
        #expect(moved.isEmpty)
        #expect(alreadyOpen.openPath == ["core"])
        #expect(alreadyOpen.selection == "core.a")
    }

    @Test func left_closes_the_submenu_and_returns_to_its_row() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.key(.down))
        _ = state.handle(.key(.right))
        #expect(state.selection == "core.a")
        #expect(state.openPath == ["core"])
        let closed = state.handle(.key(.left))
        #expect(closed == [.close(depth: 1)])
        #expect(state.openPath == [])
        #expect(state.selection == "core")
        let atRoot = state.handle(.key(.left))
        #expect(atRoot.isEmpty)
        #expect(state.openPath == [])
        #expect(state.selection == "core")

        _ = state.handle(.key(.right))
        _ = state.handle(.key(.right))
        #expect(state.openPath == ["core", "core.a"])
        #expect(state.selection == "core.a.x")
        let inner = state.handle(.key(.left))
        #expect(inner == [.close(depth: 2)])
        #expect(state.openPath == ["core"])
        #expect(state.selection == "core.a")
    }

    @Test func return_does_what_a_click_on_the_selection_does() {
        var viaKey = CascadeState(root: Self.tree)
        _ = viaKey.handle(.key(.down))
        #expect(viaKey.selection == "core")
        let keyOpen = viaKey.handle(.key(.return))
        var viaClick = CascadeState(root: Self.tree)
        let clickOpen = viaClick.handle(.click(row: "core"))
        #expect(keyOpen == clickOpen)
        #expect(keyOpen == [.open(row: "core")])
        #expect(viaKey.openPath == viaClick.openPath)
        #expect(viaKey.selection == viaClick.selection)

        var viaKeyQuit = CascadeState(root: Self.tree)
        _ = viaKeyQuit.handle(.key(.down))
        _ = viaKeyQuit.handle(.key(.down))
        _ = viaKeyQuit.handle(.key(.down))
        #expect(viaKeyQuit.selection == "quit")
        let keyActivate = viaKeyQuit.handle(.key(.return))
        var viaClickQuit = CascadeState(root: Self.tree)
        let clickActivate = viaClickQuit.handle(.click(row: "quit"))
        #expect(keyActivate == clickActivate)
        #expect(keyActivate == [.activate(row: "quit")])
        #expect(viaKeyQuit.openPath == [])
        #expect(viaKeyQuit.selection == "quit")
    }

    @Test func escape_closes_the_deepest_panel() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.click(row: "core"))
        _ = state.handle(.click(row: "core.a"))
        #expect(state.openPath == ["core", "core.a"])
        #expect(state.selection == "core.a")
        let first = state.handle(.key(.escape))
        #expect(first == [.close(depth: 2)])
        #expect(state.openPath == ["core"])
        #expect(state.selection == "core.a")
        let second = state.handle(.key(.escape))
        #expect(second == [.close(depth: 1)])
        #expect(state.openPath == [])
        #expect(state.selection == "core")
    }

    @Test func escape_at_level_zero_closes_everything() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(.key(.down))
        #expect(state.selection == "core")
        #expect(state.openPath == [])
        let effects = state.handle(.key(.escape))
        #expect(effects == [.closeAll])
        #expect(state.openPath == [])
    }

    @Test func a_real_pointer_move_after_keys_takes_the_one_selection() {
        var state = CascadeState(root: Self.tree)
        let parked = CGPoint(x: 10, y: 10)
        _ = state.handle(.pointerOn(row: "core", at: parked, submenu: nil))
        #expect(state.selection == "core")
        _ = state.handle(.key(.down))
        #expect(state.selection == "settings")
        let moved = state.handle(
            .pointerOn(row: "quit", at: CGPoint(x: 40, y: 40), submenu: nil)
        )
        #expect(moved.isEmpty)
        #expect(state.selection == "quit")
        #expect(state.openPath == [])
    }

    @Test func a_row_moving_under_a_still_pointer_does_not_take_a_keyboard_selection() {
        var state = CascadeState(root: Self.tree)
        let still = CGPoint(x: 10, y: 10)
        _ = state.handle(.pointerOn(row: "core", at: still, submenu: nil))
        #expect(state.selection == "core")
        _ = state.handle(.key(.down))
        #expect(state.selection == "settings")
        let stolen = state.handle(.pointerOn(row: "quit", at: still, submenu: nil))
        #expect(stolen.isEmpty)
        #expect(state.selection == "settings")
        #expect(state.openPath == [])
    }

    @Test func a_key_cancels_the_pending_hover() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(
            .pointerOn(row: "core", at: CGPoint(x: 10, y: 10), submenu: nil)
        )
        #expect(state.selection == "core")
        #expect(state.openPath == [])
        let keyed = state.handle(.key(.down))
        #expect(keyed == [.cancelHoverTimer])
        #expect(state.selection == "settings")
        #expect(state.openPath == [])
        let timed = state.handle(.hoverTimerFired)
        #expect(timed.isEmpty)
        #expect(state.selection == "settings")
        #expect(state.openPath == [])
    }

    private static func projectTree(_ names: [String]) -> [CascadeNode] {
        [
            CascadeNode(
                id: "running",
                isActionable: true,
                children: names.map { CascadeNode(id: "running.project.\($0)", isActionable: true) }
            ),
            CascadeNode(id: "settings", isActionable: true),
        ]
    }

    private static let submenu = CGRect(x: 400, y: 100, width: 200, height: 200)

    private static let tree: [CascadeNode] = [
        CascadeNode(
            id: "core",
            children: [
                CascadeNode(
                    id: "core.a",
                    children: [
                        CascadeNode(
                            id: "core.a.x",
                            children: [
                                CascadeNode(id: "core.a.x.y")
                            ]
                        )
                    ]
                )
            ]
        ),
        CascadeNode(id: "settings", isActionable: true),
        CascadeNode(id: "quit", isActionable: true),
    ]
}
