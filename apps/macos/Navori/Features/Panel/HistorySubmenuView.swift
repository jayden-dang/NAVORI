import AppKit
import NavoriPanel
import SwiftUI

struct HistorySubmenuView: View {
    let store: CoreStore
    @Environment(CascadeController.self) private var cascade

    var body: some View {
        CappedScroll {
            VStack(alignment: .leading, spacing: 0) {
                content
            }
        }
        .padding(MenuMetrics.pad)
        .frame(width: 350, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
        .task { await store.loadHistory() }
    }

    @ViewBuilder
    private var content: some View {
        if let error = store.historyError {
            statusLine(error)
        } else if !store.historyHasLoaded {
            statusLine("Loading…")
        } else if store.historyEntries.isEmpty {
            statusLine("Nothing here yet")
        } else {
            ForEach(Array(store.historyEntries.enumerated()), id: \.element.id) { _, entry in
                HistoryRowView(entry: entry, cascade: cascade) {
                    guard let request = entry.undoRequest else { return }
                    Task { await store.undo(request) }
                }
            }
        }
    }

    private func statusLine(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 14))
            .foregroundStyle(Color(nsColor: .secondaryLabelColor))
            .padding(.horizontal, 8)
            .frame(
                maxWidth: .infinity,
                minHeight: MenuMetrics.rowHeight,
                maxHeight: MenuMetrics.rowHeight,
                alignment: .leading
            )
            .accessibilityAddTraits(.isStaticText)
    }
}

private struct HistoryRowView: View {
    let entry: NeedsYouHistoryEntry
    let cascade: CascadeController
    let onUndo: () -> Void

    private var selected: Bool {
        cascade.selection == entry.id
    }

    private var accessibilityLabel: String {
        var parts = [entry.kind, entry.title]
        if let outcome = entry.outcome {
            parts.append(outcome)
        }
        return parts.joined(separator: ", ")
    }

    var body: some View {
        HStack(spacing: 6) {
            Text(entry.title)
                .font(.system(size: 14))
                .foregroundStyle(selected ? Color.white : Color(nsColor: .secondaryLabelColor))
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 8)
            if let outcome = entry.outcome {
                Text(outcome)
                    .font(.system(size: 13))
                    .foregroundStyle(selected ? Color.white : Color(nsColor: .secondaryLabelColor))
                    .lineLimit(1)
            }
            if entry.undoRequest != nil {
                CapsuleButton(
                    title: "Undo",
                    prominent: false,
                    selected: selected,
                    action: onUndo,
                    fontSize: 11.5,
                    minHeight: 20
                )
            }
        }
        .padding(.horizontal, 8)
        .frame(
            maxWidth: .infinity,
            minHeight: MenuMetrics.rowHeight,
            maxHeight: MenuMetrics.rowHeight,
            alignment: .leading
        )
        .background {
            if selected {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Color(nsColor: .selectedContentBackgroundColor))
            }
        }
        .contentShape(Rectangle())
        .onTapGesture { cascade.click(entry.id) }
        .onContinuousHover { phase in
            switch phase {
            case .active:
                cascade.pointer(on: entry.id, at: NSEvent.mouseLocation)
            case .ended:
                cascade.pointerLeft(entry.id)
            }
        }
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .global)
        } action: { frame in
            cascade.setRowFrame(entry.id, frame)
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(accessibilityLabel)
    }
}
