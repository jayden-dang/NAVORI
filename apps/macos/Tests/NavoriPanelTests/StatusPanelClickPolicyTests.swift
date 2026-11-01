import CoreGraphics
import NavoriPanel
import Testing

@Test func clickOnStatusButtonDoesNotDismissPanel() {
    let button = CGRect(x: 1049, y: 4, width: 24, height: 24)
    let click = CGPoint(x: 1061, y: 16)
    #expect(
        StatusPanelClickPolicy.shouldDismissPanel(
            clickScreenPoint: click,
            statusButtonScreenFrame: button
        ) == false
    )
}

@Test func clickOutsideStatusButtonDismissesPanel() {
    let button = CGRect(x: 1049, y: 4, width: 24, height: 24)
    let click = CGPoint(x: 420, y: 320)
    #expect(
        StatusPanelClickPolicy.shouldDismissPanel(
            clickScreenPoint: click,
            statusButtonScreenFrame: button
        ) == true
    )
}
