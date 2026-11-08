import AppKit
import NavoriCore
import NavoriPanel
import SwiftUI

struct DecideHeaderView: View {
    let count: Int
    let cascade: CascadeController

    private let id = "decide.header"

    var body: some View {
        HStack(spacing: 6) {
            Text("Decide")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Color(nsColor: .secondaryLabelColor))
            Spacer(minLength: 8)
            Text("\(count)")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Color(nsColor: .secondaryLabelColor))
        }
        .padding(.horizontal, 8)
        .frame(
            maxWidth: .infinity,
            minHeight: MenuMetrics.rowHeight,
            maxHeight: MenuMetrics.rowHeight,
            alignment: .leading
        )
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .global)
        } action: { frame in
            cascade.setRowFrame(id, frame)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Decide")
        .accessibilityValue("\(count)")
        .accessibilityAddTraits(.isHeader)
    }
}

struct DecideRowView: View {
    let row: DecideRow
    let cascade: CascadeController
    let store: CoreStore
    var isDimmed = false

    @State private var hovering = false
    @State private var primaryFrame = CGRect.zero
    @State private var secondaryFrame = CGRect.zero

    private var rowId: String { "decide.\(row.id)" }

    private var focused: Bool {
        cascade.selection == rowId || hovering
    }

    private var selected: Bool {
        cascade.selection == rowId
    }

    private var nowMs: Int64 {
        Int64(Date().timeIntervalSince1970 * 1000)
    }

    private var age: String {
        NeedsYouText.age(raisedAtMs: row.raisedAtMs, nowMs: nowMs)
    }

    private var failed: (choice: String, error: String)? {
        if case let .failed(choice: choice, error: error) = row.state {
            return (choice, error)
        }
        return nil
    }

    private var answeringChoice: String? {
        if case let .answering(choice: choice) = row.state {
            return choice
        }
        return nil
    }

    private var primaryTitle: String {
        failed == nil ? row.primary.label : "Retry"
    }

    private var valueText: String {
        failed?.error ?? row.reason
    }

    private var spoken: String {
        NeedsYouText.decisionSpoken(
            title: row.title,
            reason: row.reason,
            age: age,
            primary: row.primary.label,
            secondary: row.secondary.label
        )
    }

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "circle.fill")
                .font(.system(size: 13))
                .symbolRenderingMode(.monochrome)
                .foregroundStyle(
                    selected
                        ? Color.white
                        : Color(nsColor: .controlAccentColor)
                )
                .opacity(isDimmed ? 0.45 : 1)
                .frame(width: 20, height: 20)
            Text(row.title)
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(selected ? Color.white : Color(nsColor: .labelColor))
                .opacity(isDimmed ? 0.45 : 1)
                .lineLimit(1)
            Spacer(minLength: 8)
            if focused {
                buttons
            } else {
                Text(age)
                    .font(.system(size: 12))
                    .foregroundStyle(
                        selected
                            ? Color.white
                            : Color(nsColor: .secondaryLabelColor)
                    )
                    .opacity(isDimmed ? 0.45 : 1)
                    .lineLimit(1)
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
        .opacity(isDimmed ? 0.85 : 1)
        .onContinuousHover { phase in
            switch phase {
            case .active:
                hovering = true
                cascade.pointer(on: rowId, at: NSEvent.mouseLocation)
            case .ended:
                hovering = false
                cascade.pointerLeft(rowId)
            }
        }
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .global)
        } action: { frame in
            cascade.setRowFrame(rowId, frame)
        }
        .onChange(of: focused) { _, showing in
            publishButtons(showing)
        }
        .onChange(of: primaryFrame) { _, _ in
            publishButtons(focused)
        }
        .onChange(of: secondaryFrame) { _, _ in
            publishButtons(focused)
        }
        .onDisappear {
            cascade.setDecideButtons(rowId, nil)
        }
        .help(valueText)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(spoken)
        .accessibilityValue(valueText)
        .accessibilityAddTraits(.isButton)
    }

    private var buttons: some View {
        HStack(spacing: 6) {
            CapsuleButton(
                title: primaryTitle,
                prominent: true,
                selected: selected,
                action: { answerPrimary() },
                fontSize: 11.5,
                minHeight: 20,
                busy: answeringChoice == "primary",
                disabled: answeringChoice == "secondary"
            )
            .onGeometryChange(for: CGRect.self) { proxy in
                proxy.frame(in: .global)
            } action: { frame in
                primaryFrame = frame
            }
            .accessibilityHint(hint(for: row.primary))

            CapsuleButton(
                title: row.secondary.label,
                prominent: false,
                selected: selected,
                action: { answerSecondary() },
                fontSize: 11.5,
                minHeight: 20,
                busy: answeringChoice == "secondary",
                disabled: answeringChoice == "primary"
            )
            .onGeometryChange(for: CGRect.self) { proxy in
                proxy.frame(in: .global)
            } action: { frame in
                secondaryFrame = frame
            }
            .accessibilityHint(hint(for: row.secondary))
        }
    }

    private func hint(for choice: ChoiceView) -> String {
        guard choice.risk == "destructive",
              let module = choice.module,
              let action = choice.action
        else { return "" }
        return NeedsYouText.destructiveHint(module: module, action: action)
    }

    private func answerPrimary() {
        let request: String
        if let failed {
            request = failed.choice == "secondary"
                ? row.secondary.answerRequest
                : row.primary.answerRequest
        } else {
            request = row.primary.answerRequest
        }
        Task { await store.answer(request) }
    }

    private func answerSecondary() {
        Task { await store.answer(row.secondary.answerRequest) }
    }

    private func publishButtons(_ showing: Bool) {
        guard showing,
              primaryFrame != .zero,
              secondaryFrame != .zero,
              let height = cascade.panel?.frame.height
        else {
            cascade.setDecideButtons(rowId, nil)
            return
        }
        cascade.setDecideButtons(
            rowId,
            DecideButtonFrames(
                primary: toWindow(primaryFrame, height: height),
                secondary: toWindow(secondaryFrame, height: height)
            )
        )
    }

    private func toWindow(_ frame: CGRect, height: CGFloat) -> CGRect {
        CGRect(
            x: frame.minX,
            y: height - frame.minY - frame.height,
            width: frame.width,
            height: frame.height
        )
    }
}
