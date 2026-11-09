#if DEBUG
import AppKit
import SwiftUI

enum PreviewSampleRows {
    static let marker = "NAVORI-PREVIEW-SAMPLE"

    struct InboxItem: Identifiable {
        let id: String
        let lead: String
        let title: String
        let age: String
        let reason: String
        let primary: String?
        let secondary: String?
    }

    static let inbox: [InboxItem] = [
        InboxItem(
            id: "preview.inbox.stop3",
            lead: "sparkle",
            title: "Stop 3 idle Claude servers",
            age: "2m",
            reason: "so they don't sit idle in openklynt-main",
            primary: "Stop 3",
            secondary: "Keep"
        ),
        InboxItem(
            id: "preview.inbox.keep",
            lead: "sparkle",
            title: "Keep the machine awake for Codex",
            age: "8m",
            reason: "in cargo-zip · needs your approval",
            primary: "Keep 1 h",
            secondary: "Let it end"
        ),
    ]
}

struct InboxRowView: View {
    let item: PreviewSampleRows.InboxItem
    let cascade: CascadeController

    var body: some View {
        let selected = cascade.selection == item.id
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                Image(systemName: item.lead)
                    .font(.system(size: 13))
                    .symbolRenderingMode(.monochrome)
                    .foregroundStyle(selected ? Color.white : Color(nsColor: .secondaryLabelColor))
                    .frame(width: 20, height: 20)
                Text(item.title)
                    .font(.system(size: 14))
                    .foregroundStyle(selected ? Color.white : Color(nsColor: .labelColor))
                    .lineLimit(1)
                Spacer(minLength: 8)
                Text(item.age)
                    .font(.system(size: 13))
                    .foregroundStyle(selected ? Color.white : Color(nsColor: .secondaryLabelColor))
            }
            Text(item.reason)
                .font(.system(size: 12))
                .foregroundStyle(selected ? Color.white.opacity(0.9) : Color(nsColor: .secondaryLabelColor))
                .lineLimit(1)
                .padding(.leading, 26)
            if item.primary != nil || item.secondary != nil {
                HStack(spacing: 6) {
                    if let primary = item.primary {
                        CapsuleButton(title: primary, prominent: true, selected: selected)
                    }
                    if let secondary = item.secondary {
                        CapsuleButton(title: secondary, prominent: false, selected: selected)
                    }
                }
                .padding(.leading, 26)
                .padding(.top, 2)
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background {
            if selected {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Color(nsColor: .selectedContentBackgroundColor))
            }
        }
        .contentShape(Rectangle())
        .onTapGesture { cascade.click(item.id) }
        .onContinuousHover { phase in
            switch phase {
            case .active:
                cascade.pointer(on: item.id, at: NSEvent.mouseLocation)
            case .ended:
                cascade.pointerLeft(item.id)
            }
        }
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .global)
        } action: { frame in
            cascade.setRowFrame(item.id, frame)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(item.title)
        .accessibilityValue(item.reason)
        .accessibilityIdentifier(PreviewSampleRows.marker)
    }
}
#endif
