import AppKit
import NavoriPanel
import SwiftUI

struct UpdateRowView: View {
    let row: NeedsYouUpdateRow
    let cascade: CascadeController
    var onFrame: ((Int64, CGRect) -> Void)?

    private var rowId: String { "update.\(row.id)" }

    private var selected: Bool {
        cascade.selection == rowId
    }

    private var nowMs: Int64 {
        Int64(Date().timeIntervalSince1970 * 1000)
    }

    private var age: String {
        NeedsYouText.age(raisedAtMs: row.createdAtMs, nowMs: nowMs)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                Image(systemName: "bell")
                    .font(.system(size: 13))
                    .symbolRenderingMode(.monochrome)
                    .foregroundStyle(selected ? Color.white : Color(nsColor: .secondaryLabelColor))
                    .frame(width: 20, height: 20)
                Text(row.title)
                    .font(.system(size: 14))
                    .foregroundStyle(
                        selected
                            ? Color.white
                            : Color(nsColor: .labelColor).opacity(0.72)
                    )
                    .lineLimit(1)
                Spacer(minLength: 8)
            }
            Text(age)
                .font(.system(size: 12))
                .foregroundStyle(
                    selected
                        ? Color.white.opacity(0.85)
                        : Color(nsColor: .secondaryLabelColor)
                )
                .lineLimit(1)
                .padding(.leading, 26)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
        .opacity(0.85)
        .background {
            if selected {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Color(nsColor: .selectedContentBackgroundColor))
            }
        }
        .contentShape(Rectangle())
        .onTapGesture { cascade.click(rowId) }
        .onContinuousHover { phase in
            switch phase {
            case .active:
                cascade.pointer(on: rowId, at: NSEvent.mouseLocation)
            case .ended:
                cascade.pointerLeft(rowId)
            }
        }
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .global)
        } action: { frame in
            cascade.setRowFrame(rowId, frame)
            onFrame?(row.id, frame)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.title)
        .accessibilityValue(age)
    }
}
