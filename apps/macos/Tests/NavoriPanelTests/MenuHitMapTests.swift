import CoreGraphics
import NavoriPanel
import Testing

@Suite
struct MenuHitMapTests {
    private static let width = MenuMetrics.level0Width
    private static let row = MenuMetrics.rowHeight
    private static let pad = MenuMetrics.pad

    private static let withoutCore: [CascadeNode] = [
        CascadeNode(id: "running", isActionable: true),
        CascadeNode(id: "settings", isActionable: true),
        CascadeNode(id: "about", isActionable: true),
        CascadeNode(id: "quit", isActionable: true),
    ]

    private static let withCore: [CascadeNode] = [
        CascadeNode(id: "running", isActionable: true),
        CascadeNode(id: "core", isActionable: true),
        CascadeNode(id: "settings", isActionable: true),
        CascadeNode(id: "about", isActionable: true),
        CascadeNode(id: "quit", isActionable: true),
    ]

    @Test func the_footer_reads_from_the_bottom_of_the_window() {
        #expect(hit(Self.pad + 1, in: Self.withoutCore) == "quit")
        #expect(hit(Self.pad + Self.row + 1, in: Self.withoutCore) == "about")
        #expect(hit(Self.pad + 2 * Self.row + 1, in: Self.withoutCore) == "settings")
    }

    @Test func a_core_row_sits_above_the_divider_when_the_tree_includes_it() {
        let coreY = Self.pad + 3 * Self.row + MenuMetrics.divider + 1
        #expect(hit(coreY, in: Self.withCore) == "core")
        #expect(hit(coreY, in: Self.withoutCore) == nil)
    }

    @Test func running_is_not_a_first_mouse_hit() {
        let runningY = Self.pad + 4 * Self.row + MenuMetrics.divider + 1
        #expect(hit(runningY, in: Self.withCore) == nil)
    }

    @Test func a_click_in_the_side_pad_hits_nothing() {
        let point = CGPoint(x: 2, y: Self.pad + 1)
        #expect(MenuHitMap.rowId(at: point, width: Self.width, tree: Self.withoutCore) == nil)
    }

    @Test func arrows_return_and_escape_map_to_cascade_keys() {
        #expect(CascadeKey.fromKeyCode(126) == .up)
        #expect(CascadeKey.fromKeyCode(125) == .down)
        #expect(CascadeKey.fromKeyCode(123) == .left)
        #expect(CascadeKey.fromKeyCode(124) == .right)
        #expect(CascadeKey.fromKeyCode(36) == .return)
        #expect(CascadeKey.fromKeyCode(76) == .return)
        #expect(CascadeKey.fromKeyCode(53) == .escape)
        #expect(CascadeKey.fromKeyCode(0) == nil)
    }

    @Test func testHitMapFindsRowsAboveRunningAndDecideButtons() {
        let tree: [CascadeNode] = [
            CascadeNode(id: "decide.1", isActionable: true),
            CascadeNode(id: "needs", isActionable: true),
            CascadeNode(id: "running", isActionable: true),
            CascadeNode(id: "settings", isActionable: true),
            CascadeNode(id: "about", isActionable: true),
            CascadeNode(id: "quit", isActionable: true),
        ]
        // Bottom-up: quit, about, settings, running (skipped), needs, decide.1
        let needsY = Self.pad + 4 * Self.row + 1
        let decideY = Self.pad + 5 * Self.row + 1
        #expect(hit(needsY, in: tree) == "needs")
        #expect(hit(decideY, in: tree) == "decide.1")
        #expect(hit(Self.pad + 3 * Self.row + 1, in: tree) == nil) // running

        let primary = CGRect(x: 280, y: decideY - 1, width: 40, height: 20)
        let secondary = CGRect(x: 230, y: decideY - 1, width: 40, height: 20)
        let buttons = ["decide.1": DecideButtonFrames(primary: primary, secondary: secondary)]
        #expect(
            MenuHitMap.hit(
                at: CGPoint(x: primary.midX, y: primary.midY),
                width: Self.width,
                tree: tree,
                decideButtons: buttons
            ) == .decidePrimary(row: "decide.1")
        )
        #expect(
            MenuHitMap.hit(
                at: CGPoint(x: secondary.midX, y: secondary.midY),
                width: Self.width,
                tree: tree,
                decideButtons: buttons
            ) == .decideSecondary(row: "decide.1")
        )
        #expect(
            MenuHitMap.hit(
                at: CGPoint(x: 20, y: decideY),
                width: Self.width,
                tree: tree,
                decideButtons: buttons
            ) == .row("decide.1")
        )
    }

    private func hit(_ y: CGFloat, in tree: [CascadeNode]) -> String? {
        MenuHitMap.rowId(at: CGPoint(x: 20, y: y), width: Self.width, tree: tree)
    }
}
