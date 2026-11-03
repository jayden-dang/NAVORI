#if DEBUG
import NavoriPanel
import SwiftUI

/// Debug-only Samples: each icon state can be held on the real status item to judge it.
/// The row dispatches no Action; it only calls the closures it is given.
enum PreviewIconSamples {
    static let marker = "NAVORI-ICON-SAMPLE"

    struct Entry: Identifiable {
        let title: String
        let state: StatusIconState
        var id: String { title }
    }

    static let entries: [Entry] = {
        func entry(_ title: String, _ glyph: StatusIconGlyph, _ attention: Bool, _ words: String) -> Entry {
            let description = attention ? words + ". A decision is waiting" : words
            return Entry(
                title: title,
                state: StatusIconState(glyph: glyph, attention: attention, description: description)
            )
        }
        let problem = "Can't hold this Mac awake. macOS refused the request. Navori keeps trying"
        var all: [Entry] = []
        for attention in [false, true] {
            let suffix = attention ? " + attention" : ""
            all.append(entry("Normal" + suffix, .normal, attention, "Navori"))
            all.append(entry("Keeping awake" + suffix, .lease, attention, "Keeping this Mac awake"))
            all.append(entry("AI running" + suffix, .aiRunning, attention, "Navori AI is running"))
            all.append(entry("Problem" + suffix, .problem, attention, problem))
        }
        all.append(entry("Core not running", .coreDown, false, "Core not running"))
        all.append(entry("Starting (dimmed)", .starting, false, "Starting"))
        return all
    }()
}

struct PreviewIconSampleList: View {
    let setIconSample: (StatusIconState?) -> Void
    let playBeat: () -> Void
    @State private var chosen: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Icon samples")
                .font(.system(size: 14))
                .foregroundStyle(Color(nsColor: .labelColor))
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
            ForEach(PreviewIconSamples.entries) { entry in
                button(entry.title, selected: chosen == entry.id) {
                    chosen = entry.id
                    setIconSample(entry.state)
                }
            }
            button("Play beat", selected: false, action: playBeat)
            button("Turn off", selected: false) {
                chosen = nil
                setIconSample(nil)
            }
        }
        .accessibilityIdentifier(PreviewIconSamples.marker)
    }

    private func button(_ title: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack {
                Text(title)
                    .font(.system(size: 13))
                    .foregroundStyle(Color(nsColor: .labelColor))
                Spacer(minLength: 8)
                if selected {
                    Image(systemName: "checkmark")
                        .font(.system(size: 11))
                        .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
#endif
