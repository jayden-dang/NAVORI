import NavoriPanel
import SwiftUI

/// What a Cascade shows.
struct CascadeContent {
    let tree: () -> [CascadeNode]
    let root: AnyView
    let submenu: (String) -> AnyView
    let activate: (String) -> Void
}
