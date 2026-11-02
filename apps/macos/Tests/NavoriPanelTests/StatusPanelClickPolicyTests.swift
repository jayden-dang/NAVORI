import CoreGraphics
import NavoriPanel
import Testing

@Test func clickOnStatusButtonDoesNotDismissPanel() {
    let button = CGRect(x: 1049, y: 4, width: 24, height: 24)
    let click = CGPoint(x: 1061, y: 16)
    #expect(
        StatusPanelClickPolicy.shouldDismissPanel(
            clickScreenPoint: click,
            statusButtonScreenFrame: button,
            cascadeFrames: []
        ) == false
    )
}

@Test func clickOutsideStatusButtonDismissesPanel() {
    let button = CGRect(x: 1049, y: 4, width: 24, height: 24)
    let click = CGPoint(x: 420, y: 320)
    #expect(
        StatusPanelClickPolicy.shouldDismissPanel(
            clickScreenPoint: click,
            statusButtonScreenFrame: button,
            cascadeFrames: []
        ) == true
    )
}

@Test func a_click_inside_an_open_panel_does_not_dismiss() {
    let button = CGRect(x: 1049, y: 4, width: 24, height: 24)
    let panel = CGRect(x: 1000, y: 400, width: 360, height: 300)
    let click = CGPoint(x: 1100, y: 500)
    #expect(
        StatusPanelClickPolicy.shouldDismissPanel(
            clickScreenPoint: click,
            statusButtonScreenFrame: button,
            cascadeFrames: [panel]
        ) == false
    )
}

@Test func a_click_outside_every_panel_and_the_button_dismisses() {
    let button = CGRect(x: 1049, y: 4, width: 24, height: 24)
    let panel = CGRect(x: 1000, y: 400, width: 360, height: 300)
    let click = CGPoint(x: 420, y: 320)
    #expect(
        StatusPanelClickPolicy.shouldDismissPanel(
            clickScreenPoint: click,
            statusButtonScreenFrame: button,
            cascadeFrames: [panel]
        ) == true
    )
}
