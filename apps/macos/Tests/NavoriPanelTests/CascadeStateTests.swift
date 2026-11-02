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
        #expect(armed.contains(.armHoverTimer))
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
        #expect(effects == [.armHoverTimer])
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
        #expect(effects == [.close(depth: 1), .armHoverTimer])
        #expect(state.selection == "settings")
        #expect(state.openPath == [])
    }

    @Test func every_new_pointer_position_rearms_the_timer() {
        var state = CascadeState(root: Self.tree)
        let first = state.handle(
            .pointerOn(row: "core", at: CGPoint(x: 100, y: 200), submenu: nil)
        )
        #expect(first == [.armHoverTimer])
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
        #expect(second == [.armHoverTimer])
        #expect(state.selection == "core")
        #expect(state.openPath == ["core"])
        let third = state.handle(
            .pointerOn(
                row: "quit",
                at: CGPoint(x: 260, y: 205),
                submenu: Self.submenu
            )
        )
        #expect(third == [.armHoverTimer])
        #expect(state.selection == "core")
        #expect(state.openPath == ["core"])
    }

    @Test func the_same_pointer_position_twice_changes_nothing() {
        var state = CascadeState(root: Self.tree)
        let point = CGPoint(x: 100, y: 200)
        let first = state.handle(.pointerOn(row: "core", at: point, submenu: nil))
        #expect(first == [.armHoverTimer])
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
        #expect(leftQuit == [.cancelHoverTimer])
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
        #expect(armed == [.armHoverTimer])
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
        #expect(armed == [.armHoverTimer])
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
        #expect(pointed == [.armHoverTimer])
        #expect(state.selection == nil)
        #expect(state.openPath == [])
        let timed = state.handle(.hoverTimerFired)
        #expect(timed.isEmpty)
        #expect(state.selection == nil)
        let clicked = state.handle(.click(row: "heading"))
        #expect(clicked == [.cancelHoverTimer])
        #expect(state.selection == nil)
        #expect(state.openPath == [])
    }

    @Test func a_click_cancels_the_pending_hover() {
        var state = CascadeState(root: Self.tree)
        _ = state.handle(
            .pointerOn(row: "settings", at: CGPoint(x: 10, y: 10), submenu: nil)
        )
        #expect(state.selection == "settings")
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
