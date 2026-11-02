import CoreGraphics
import NavoriPanel
import Testing

@Suite
struct CascadePlacementTests {
    @Test func a_submenu_opens_to_the_right_of_its_parent_aligned_to_its_row() {
        let visible = CGRect(x: 0, y: 0, width: 1800, height: 900)
        let parent = CGRect(x: 1000, y: 400, width: 360, height: 300)
        let row = CGRect(x: 1000, y: 622, width: 360, height: 28)
        let size = CGSize(width: 360, height: 200)
        let frame = CascadePlacement.frame(
            parent: parent,
            row: row,
            size: size,
            visible: visible
        )
        #expect(frame == CGRect(x: 1364, y: 450, width: 360, height: 200))
    }

    @Test func a_submenu_that_would_cross_the_right_edge_opens_on_the_left() {
        let visible = CGRect(x: 0, y: 0, width: 1500, height: 900)
        let parent = CGRect(x: 1000, y: 400, width: 360, height: 300)
        let row = CGRect(x: 1000, y: 622, width: 360, height: 28)
        let size = CGSize(width: 360, height: 200)
        let frame = CascadePlacement.frame(
            parent: parent,
            row: row,
            size: size,
            visible: visible
        )
        #expect(frame.origin.x == 636)
        #expect(frame == CGRect(x: 636, y: 450, width: 360, height: 200))
    }

    @Test func a_submenu_near_the_bottom_shifts_up_to_stay_on_screen() {
        let visible = CGRect(x: 0, y: 0, width: 1800, height: 900)
        let parent = CGRect(x: 1000, y: 0, width: 360, height: 150)
        let row = CGRect(x: 1000, y: 52, width: 360, height: 28)
        let size = CGSize(width: 360, height: 200)
        let frame = CascadePlacement.frame(
            parent: parent,
            row: row,
            size: size,
            visible: visible
        )
        #expect(frame == CGRect(x: 1364, y: 0, width: 360, height: 200))
    }

    @Test func a_submenu_taller_than_the_screen_is_limited_to_the_visible_height() {
        let visible = CGRect(x: 0, y: 0, width: 1800, height: 900)
        let parent = CGRect(x: 1000, y: 400, width: 360, height: 300)
        let row = CGRect(x: 1000, y: 622, width: 360, height: 28)
        let size = CGSize(width: 360, height: 1000)
        let frame = CascadePlacement.frame(
            parent: parent,
            row: row,
            size: size,
            visible: visible
        )
        #expect(frame == CGRect(x: 1364, y: 0, width: 360, height: 900))
    }
}
