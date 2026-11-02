import CoreGraphics
import NavoriPanel
import Testing

@Suite
struct SafeDiagonalTests {
    @Test func a_point_toward_the_submenu_is_inside() {
        let submenu = CGRect(x: 400, y: 100, width: 200, height: 200)
        let anchor = CGPoint(x: 100, y: 200)
        let toward = CGPoint(x: 250, y: 200)
        #expect(SafeDiagonal.contains(toward, from: anchor, toward: submenu))
    }

    @Test func a_point_moving_away_is_outside() {
        let submenu = CGRect(x: 400, y: 100, width: 200, height: 200)
        let anchor = CGPoint(x: 100, y: 200)
        let away = CGPoint(x: 50, y: 200)
        #expect(SafeDiagonal.contains(away, from: anchor, toward: submenu) == false)
    }

    @Test func the_triangle_points_left_for_a_flipped_submenu() {
        let submenu = CGRect(x: 0, y: 100, width: 200, height: 200)
        let anchor = CGPoint(x: 400, y: 200)
        let towardLeft = CGPoint(x: 300, y: 200)
        let awayRight = CGPoint(x: 500, y: 200)
        #expect(SafeDiagonal.contains(towardLeft, from: anchor, toward: submenu))
        #expect(SafeDiagonal.contains(awayRight, from: anchor, toward: submenu) == false)
    }
}
