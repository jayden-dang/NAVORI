import NavoriPanel
import SwiftUI

/// The third level: how long to keep the Mac awake.
struct KeepAwakeSubmenuView: View {
    let store: CoreStore
    @Environment(CascadeController.self) private var cascade

    var body: some View {
        let state = store.awakeSubmenu
        VStack(alignment: .leading, spacing: 0) {
            ForEach(KeepAwakeChoice.all, id: \.id) { choice in
                MenuRowView(
                    row: MenuRow(
                        id: choice.id,
                        icon: nil,
                        title: choice.title,
                        value: nil,
                        dot: nil,
                        opensSubmenu: false,
                        shortcut: nil,
                        isChecked: state.isChecked(choice)
                    ),
                    cascade: cascade
                )
            }
        }
        .padding(MenuMetrics.pad)
        .frame(width: 180, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
    }
}
