import CoreGraphics
import NavoriPanel
import Testing

@Suite
struct CascadeRowFramesTests {
    // A 141 pt panel with the row 26 pt tall, 31 pt from its top, in SwiftUI's top-left coordinates.
    private static let row = CGRect(x: 5, y: 31, width: 350, height: 26)
    private static let wideDisplay = CGRect(x: 2877, y: 1268, width: 360, height: 141)
    private static let leftDisplay = CGRect(x: -400, y: 1101, width: 360, height: 141)

    @Test func a_row_is_placed_from_the_top_of_its_panel() {
        var frames = CascadeRowFrames()
        frames.record("core", Self.row)
        #expect(frames.screenFrame(of: "core", in: Self.wideDisplay)
            == CGRect(x: 2882, y: 1352, width: 350, height: 26))
    }

    @Test func a_row_follows_its_panel_to_another_display_without_being_recorded_again() {
        var frames = CascadeRowFrames()
        frames.record("core", Self.row)
        _ = frames.screenFrame(of: "core", in: Self.wideDisplay)
        let moved = frames.screenFrame(of: "core", in: Self.leftDisplay)
        #expect(moved == CGRect(x: -395, y: 1185, width: 350, height: 26))
        #expect(moved.map { Self.leftDisplay.contains($0) } == true)
    }

    @Test func a_row_recorded_before_its_panel_was_placed_still_lands_inside_it() {
        var frames = CascadeRowFrames()
        frames.record("awake.keep", CGRect(x: 5, y: 5, width: 350, height: 26))
        let placed = CGRect(x: 736, y: 650, width: 360, height: 120)
        #expect(frames.screenFrame(of: "awake.keep", in: placed)
            == CGRect(x: 741, y: 739, width: 350, height: 26))
    }

    @Test func a_row_never_recorded_has_no_frame() {
        #expect(CascadeRowFrames().screenFrame(of: "core", in: Self.wideDisplay) == nil)
    }
}
