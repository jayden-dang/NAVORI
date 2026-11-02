import CoreGraphics
import Foundation

/// Decides whether an outside-click monitor should dismiss the status panel.
public enum StatusPanelClickPolicy {
    /// Status-item clicks are owned by the button's toggle action; dismissing
    /// them here races mouse-up and re-opens the panel.
    public static func shouldDismissPanel(
        clickScreenPoint: CGPoint,
        statusButtonScreenFrame: CGRect,
        cascadeFrames: [CGRect]
    ) -> Bool {
        if statusButtonScreenFrame.contains(clickScreenPoint) {
            return false
        }
        return !cascadeFrames.contains { $0.contains(clickScreenPoint) }
    }
}
