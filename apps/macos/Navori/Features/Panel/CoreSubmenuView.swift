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
        .padding(MenuMetrics.pad)
        .frame(width: MenuMetrics.level0Width, alignment: .topLeading)
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
                            .fill(statusDotColor(ModuleDot.color(state: module.state)))
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
                    let age = Self.relative(event.tsMs)
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Circle()
                            .fill(Color(nsColor: .secondaryLabelColor))
                            .frame(width: 7, height: 7)
                        Text(event.summary)
                            .font(.system(size: 13))
                            .foregroundStyle(Color(nsColor: .labelColor))
                            .lineLimit(2)
                        Spacer(minLength: 4)
                        Text(age)
                            .font(.system(size: 12))
                            .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                    }
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(event.summary)
                    .accessibilityValue(spoken(event, age: age))
                }
            }
        }
    }

    private func spoken(_ event: EventRow, age: String) -> String {
        guard event.severity.isEmpty == false else { return age }
        return "\(event.severity), \(age)"
    }

    private static func relative(_ ms: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
        return date.formatted(.relative(presentation: .numeric, unitsStyle: .narrow))
    }
}
