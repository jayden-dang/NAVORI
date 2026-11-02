import AppKit
import NavoriPanel
import SwiftUI

struct MenuRow {
    let id: String
    let icon: String?
    let title: String
    let value: String?
    let dot: StatusDot?
    let opensSubmenu: Bool
    let shortcut: KeyEquivalent?
}

struct MenuRowView: View {
    let row: MenuRow
    let cascade: CascadeController

    init(row: MenuRow, cascade: CascadeController) {
        self.row = row
        self.cascade = cascade
    }

    var body: some View {
        let selected = cascade.selection == row.id
        Button(action: { cascade.click(row.id) }) {
            HStack(spacing: 6) {
                lead(selected: selected)
                Text(row.title)
                    .font(.system(size: 14))
                    .foregroundStyle(selected ? Color.white : Color(nsColor: .labelColor))
                    .lineLimit(1)
                if row.id == "core", AppEnvironment.isDev {
                    Text("DEV")
                        .font(.caption2.weight(.bold))
                        .padding(.horizontal, 5)
                        .padding(.vertical, 1)
                        .foregroundStyle(selected ? Color.white : Color(nsColor: .labelColor))
                        .background(Color(nsColor: .systemOrange).opacity(0.25), in: Capsule())
                }
                Spacer(minLength: 8)
                if let dot = row.dot {
                    Circle()
                        .fill(dotColor(dot))
                        .frame(width: 7, height: 7)
                }
                if let value = row.value {
                    Text(value)
                        .font(.system(size: 13))
                        .foregroundStyle(selected ? Color.white : Color(nsColor: .secondaryLabelColor))
                        .lineLimit(1)
                }
                if let shortcut = row.shortcut {
                    Text("⌘\(String(shortcut.character).uppercased())")
                        .font(.system(size: 13))
                        .foregroundStyle(selected ? Color.white : Color(nsColor: .secondaryLabelColor))
                }
                if row.opensSubmenu {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 11, weight: .semibold))
                        .symbolRenderingMode(.monochrome)
                        .foregroundStyle(selected ? Color.white : Color(nsColor: .secondaryLabelColor))
                }
            }
            .padding(.horizontal, 8)
            .frame(maxWidth: .infinity, minHeight: 26, maxHeight: 26, alignment: .leading)
            .background {
                if selected {
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(Color(nsColor: .selectedContentBackgroundColor))
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .focusEffectDisabled()
        .modifier(ShortcutModifier(shortcut: row.shortcut))
        .onContinuousHover { phase in
            switch phase {
            case .active:
                cascade.pointer(on: row.id, at: NSEvent.mouseLocation)
            case .ended:
                cascade.pointerLeft(row.id)
            }
        }
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .global)
        } action: { frame in
            cascade.setRowFrame(row.id, frame)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.title)
        .accessibilityValue(row.value ?? "")
        .accessibilityHint(row.opensSubmenu ? "Opens a submenu" : "")
        .accessibilityAddTraits(.isButton)
    }

    @ViewBuilder
    private func lead(selected: Bool) -> some View {
        ZStack {
            if let icon = row.icon {
                Image(systemName: icon)
                    .font(.system(size: 13))
                    .symbolRenderingMode(.monochrome)
                    .foregroundStyle(selected ? Color.white : Color(nsColor: .secondaryLabelColor))
            }
        }
        .frame(width: 20, height: 20)
    }

    private func dotColor(_ dot: StatusDot) -> Color {
        switch dot {
        case .green: Color(nsColor: .systemGreen)
        case .yellow: Color(nsColor: .systemYellow)
        case .orange: Color(nsColor: .systemOrange)
        case .red: Color(nsColor: .systemRed)
        case .gray: Color(nsColor: .systemGray)
        }
    }
}

private struct ShortcutModifier: ViewModifier {
    let shortcut: KeyEquivalent?

    func body(content: Content) -> some View {
        if let shortcut {
            content.keyboardShortcut(shortcut)
        } else {
            content
        }
    }
}
