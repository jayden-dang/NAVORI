import AppKit
import NavoriPanel
import SwiftUI

struct CoreSubmenuView: View {
    let store: CoreStore

    init(store: CoreStore) {
        self.store = store
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ModulesSection(modules: store.moduleRows)
            ActivitySection(events: store.eventRows)
        }
        .padding(5)
        .frame(width: 360, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
    }
}

private struct ModulesSection: View {
    let modules: [ModuleRow]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            SectionTitle("Core")
            ForEach(modules) { module in
                VStack(alignment: .leading, spacing: 1) {
                    HStack(spacing: 8) {
                        Circle()
                            .fill(dotColor(ModuleDot.color(state: module.state)))
                            .frame(width: 7, height: 7)
                        Text(module.id)
                            .font(.system(size: 13))
                            .foregroundStyle(Color(nsColor: .labelColor))
                            .lineLimit(1)
                        Spacer(minLength: 8)
                        Text(label(module))
                            .font(.system(size: 13))
                            .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                            .lineLimit(1)
                    }
                    if let detail = module.detail, !detail.isEmpty {
                        Text(detail)
                            .font(.system(size: 12))
                            .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                            .padding(.leading, 15)
                    }
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(module.id)
                .accessibilityValue(spoken(module))
            }
        }
    }

    private func label(_ module: ModuleRow) -> String {
        module.restarts > 0 ? "\(module.state) · \(module.restarts) restarts" : module.state
    }

    private func spoken(_ module: ModuleRow) -> String {
        guard let detail = module.detail, !detail.isEmpty else { return label(module) }
        return "\(label(module)), \(detail)"
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

private struct ActivitySection: View {
    let events: [EventRow]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            SectionTitle("Activity")
            if events.isEmpty {
                Text("Nothing yet.")
                    .font(.system(size: 13))
                    .foregroundStyle(Color(nsColor: .secondaryLabelColor))
            } else {
                ForEach(Array(events.prefix(8))) { event in
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Image(systemName: icon(event.severity))
                            .foregroundStyle(tint(event.severity))
                            .font(.system(size: 12))
                            .frame(width: 12)
                        Text(event.summary)
                            .font(.system(size: 13))
                            .foregroundStyle(Color(nsColor: .labelColor))
                            .lineLimit(2)
                        Spacer(minLength: 4)
                        Text(Self.relative(event.tsMs))
                            .font(.system(size: 12))
                            .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                    }
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(event.summary)
                    .accessibilityValue(Self.relative(event.tsMs))
                }
            }
        }
    }

    private func icon(_ severity: String) -> String {
        switch severity {
        case "error": "xmark.octagon.fill"
        case "warn": "exclamationmark.triangle.fill"
        default: "circle.fill"
        }
    }

    private func tint(_ severity: String) -> Color {
        switch severity {
        case "error": Color(nsColor: .systemRed)
        case "warn": Color(nsColor: .systemOrange)
        default: Color(nsColor: .secondaryLabelColor)
        }
    }

    private static func relative(_ ms: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
        return date.formatted(.relative(presentation: .numeric, unitsStyle: .narrow))
    }
}
