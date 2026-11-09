import AppKit
import SwiftUI

struct CapsuleButton: View {
    let title: String
    var prominent = false
    var selected = false
    var action: () -> Void = {}
    var fontSize: CGFloat = 12.5
    var minHeight: CGFloat = 24
    var busy = false
    var disabled = false
    var systemImage: String?

    var body: some View {
        Button(action: action) {
            Group {
                if busy {
                    ProgressView()
                        .controlSize(.small)
                } else {
                    HStack(spacing: 4) {
                        if let systemImage {
                            Image(systemName: systemImage)
                                .font(.system(size: fontSize - 1, weight: .medium))
                        }
                        Text(title)
                            .font(.system(size: fontSize, weight: .medium))
                    }
                    .foregroundStyle(label)
                }
            }
            .padding(.horizontal, systemImage == nil ? 10 : 8)
            .frame(minHeight: minHeight)
            .background(Capsule().fill(fill))
        }
        .buttonStyle(.plain)
        .focusEffectDisabled()
        .disabled(disabled || busy)
        .opacity(disabled ? 0.45 : 1)
    }

    private var label: Color {
        if prominent {
            selected ? Color(nsColor: .controlAccentColor) : Color.white
        } else {
            selected ? Color.white : Color(nsColor: .labelColor)
        }
    }

    private var fill: Color {
        if prominent {
            selected ? Color.white : Color(nsColor: .controlAccentColor)
        } else {
            selected ? Color.white.opacity(0.22) : Color.primary.opacity(0.08)
        }
    }
}
