import NavoriPanel
import SwiftUI

struct CascadeContent {
    let tree: () -> [CascadeNode]
    let root: AnyView
    let submenu: (String) -> AnyView
    let activate: (String) -> Void
    let caughtUp: () -> Bool
}
