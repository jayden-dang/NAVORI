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

    @Test func a_third_panel_opens_beside_its_parent_not_level_zero() {
        let visible = CGRect(x: 0, y: 0, width: 1800, height: 900)
        let levelZero = CGRect(x: 100, y: 400, width: 360, height: 300)
        let parent = CGRect(x: 464, y: 450, width: 360, height: 200)
        let row = CGRect(x: 464, y: 572, width: 360, height: 28)
        let size = CGSize(width: 360, height: 180)
        let third = CascadePlacement.submenuFrame(
            levelZero: levelZero,
            ancestorFrames: [parent],
            row: row,
            size: size,
            visible: visible
        )
        let againstLevelZero = CascadePlacement.frame(
            parent: levelZero,
            row: row,
            size: size,
            visible: visible
        )
        #expect(third == CGRect(x: 828, y: 420, width: 360, height: 180))
        #expect(third != againstLevelZero)
    }

    @Test func a_third_panel_keeps_opening_left_once_its_parent_opened_left() {
        let visible = CGRect(x: 0, y: 0, width: 1512, height: 900)
        let levelZero = CGRect(x: 1100, y: 600, width: 360, height: 200)
        let parent = CGRect(x: 736, y: 650, width: 360, height: 120)
        let row = CGRect(x: 736, y: 742, width: 360, height: 26)
        let third = CascadePlacement.submenuFrame(
            levelZero: levelZero,
            ancestorFrames: [parent],
            row: row,
            size: CGSize(width: 180, height: 218),
            visible: visible
        )
        #expect(third == CGRect(x: 552, y: 550, width: 180, height: 218))
        #expect(third.intersects(levelZero) == false)
    }

    @Test func a_third_panel_with_no_room_on_the_left_opens_on_the_right() {
        let visible = CGRect(x: 0, y: 0, width: 1000, height: 900)
        let levelZero = CGRect(x: 400, y: 600, width: 360, height: 200)
        let parent = CGRect(x: 36, y: 650, width: 360, height: 120)
        let row = CGRect(x: 36, y: 742, width: 360, height: 26)
        let third = CascadePlacement.submenuFrame(
            levelZero: levelZero,
            ancestorFrames: [parent],
            row: row,
            size: CGSize(width: 180, height: 218),
            visible: visible
        )
        #expect(third.minX == parent.maxX + CascadePlacement.gap)
    }

    @Test func a_level_zero_panel_that_fits_keeps_its_fitting_height() {
        #expect(CascadePlacement.levelZeroHeight(fitting: 180, visibleHeight: 900) == 180)
    }

    @Test func a_level_zero_panel_taller_than_the_visible_frame_uses_that_height() {
        #expect(CascadePlacement.levelZeroHeight(fitting: 1400, visibleHeight: 400) == 400)
    }

    @Test func a_level_zero_panel_is_not_given_a_two_hundred_point_floor() {
        #expect(CascadePlacement.levelZeroHeight(fitting: 36, visibleHeight: 900) == 36)
    }
}
