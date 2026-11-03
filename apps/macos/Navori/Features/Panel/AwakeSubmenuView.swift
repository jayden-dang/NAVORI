import AppKit
import NavoriPanel
import SwiftUI

struct AwakeSubmenuView: View {
    let store: CoreStore
    @Environment(CascadeController.self) private var cascade

    private var state: AwakeSubmenuState {
        AwakeSubmenuState(
            status: store.awakeBlockStatus,
            leases: store.awakeLeases,
            displayHeld: store.displayHeld,
            displayWanted: store.displayWanted,
            actionFailure: store.awakeActionMessage
        )
    }

    var body: some View {
        let state = state
        VStack(alignment: .leading, spacing: 0) {
            MenuRowView(row: command("awake.hold_hour", "Keep awake for 1 hour"), cascade: cascade)
            MenuRowView(row: command("awake.hold_until_off", "Keep awake until I turn it off"), cascade: cascade)
            StatusBlockView(title: state.statusTitle, detail: state.statusDetail)
            MenuRowView(row: command("awake.display", state.displayLabel), cascade: cascade)
            MenuRowView(
                row: command("awake.release_all", "Release all", dimmed: state.releaseAllEnabled == false),
                cascade: cascade
            )
            if state.holdingRows.isEmpty == false {
                VStack(alignment: .leading, spacing: 6) {
                    SectionTitle("Holding")
                    ForEach(Array(state.holdingRows.enumerated()), id: \.offset) { _, row in
                        HoldingLine(row: row)
                    }
                }
                .padding(.horizontal, 8)
                .padding(.top, 8)
                .padding(.bottom, 4)
            }
        }
        .padding(5)
        .frame(width: 360, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
    }

    private func command(_ id: String, _ title: String, dimmed: Bool = false) -> MenuRow {
        MenuRow(
            id: id,
            icon: nil,
            title: title,
            value: nil,
            dot: nil,
            opensSubmenu: false,
            shortcut: nil,
            isDimmed: dimmed
        )
    }
}

struct StatusBlockView: View {
    let title: String
    let detail: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.system(size: 14))
                .foregroundStyle(Color(nsColor: .labelColor))
            if detail.isEmpty == false {
                Text(detail)
                    .font(.system(size: 12))
                    .foregroundStyle(Color(nsColor: .secondaryLabelColor))
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityValue(detail)
    }
}

private struct HoldingLine: View {
    let row: HoldingRow

    private var until: String? {
        row.line2.hasPrefix("until ") ? row.line2 : nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            HStack(spacing: 8) {
                Text(row.line1)
                    .font(.system(size: 14))
                    .foregroundStyle(Color(nsColor: .labelColor))
                    .lineLimit(1)
                Spacer(minLength: 8)
                if let until {
                    Text(until)
                        .font(.system(size: 13))
                        .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                        .lineLimit(1)
                }
            }
            if until == nil, row.line2.isEmpty == false {
                Text(row.line2)
                    .font(.system(size: 11.5, design: .monospaced))
                    .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                    .lineLimit(1)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.accessibilitySentence)
    }
}
