import NavoriPanel
import Testing

@Suite
struct CoreRowStateTests {
    @Test func each_status_has_its_label_dot_and_submenu() {
        let cases: [(CoreStatus, String, StatusDot?, Bool)] = [
            (.starting, "Starting", nil, false),
            (.running, "Running", .green, true),
            (.needsAttention, "Needs attention", .orange, true),
            (.notRunning, "Not running", .red, false),
        ]
        for (status, label, dot, hasSubmenu) in cases {
            let row = CoreRowState(status: status)
            #expect(row.label == label)
            #expect(row.dot == dot)
            #expect(row.hasSubmenu == hasSubmenu)
        }
    }

    @Test func the_production_menu_shows_the_core_row_only_when_it_needs_attention_or_is_down() {
        let cases: [(CoreStatus, Bool)] = [
            (.starting, false),
            (.running, false),
            (.needsAttention, true),
            (.notRunning, true),
        ]
        for (status, visible) in cases {
            #expect(CoreRowState.isVisible(status: status, inDevProfile: false) == visible)
        }
    }

    @Test func the_dev_menu_always_shows_the_core_row() {
        for status in [CoreStatus.starting, .running, .needsAttention, .notRunning] {
            #expect(CoreRowState.isVisible(status: status, inDevProfile: true))
        }
    }

    @Test func module_dots_follow_the_module_state() {
        let cases: [(String, StatusDot)] = [
            ("running", .green),
            ("starting", .yellow),
            ("restarting", .yellow),
            ("failed", .red),
            ("stopped", .gray),
        ]
        for (state, expected) in cases {
            #expect(ModuleDot.color(state: state) == expected)
        }
    }

    @Test func each_health_maps_to_the_core_row_status() {
        #expect(CoreStatus(health: .starting) == .starting)
        #expect(CoreStatus(health: .ok) == .running)
        #expect(CoreStatus(health: .degraded) == .needsAttention)
        #expect(CoreStatus(health: .down) == .notRunning)
    }

    private func status(
        coreFailed: Bool = false,
        hasSnapshot: Bool = true,
        modules: [String] = ["running"],
        notice: Bool = false
    ) -> CoreStatus {
        CoreStatus(health: CoreHealth(
            coreFailed: coreFailed,
            hasSnapshot: hasSnapshot,
            moduleStates: modules,
            hasConfigNotice: notice
        ))
    }

    @Test func a_config_notice_alone_needs_attention() {
        #expect(status(notice: true) == .needsAttention)
    }

    @Test func every_module_running_without_a_notice_is_running() {
        #expect(status(modules: ["running", "running"], notice: false) == .running)
    }

    @Test func a_failed_or_restarting_module_needs_attention() {
        #expect(status(modules: ["running", "failed"]) == .needsAttention)
        #expect(status(modules: ["restarting"]) == .needsAttention)
    }

    @Test func a_starting_or_stopped_module_stays_running() {
        #expect(status(modules: ["starting", "stopped"]) == .running)
    }

    @Test func a_failed_core_is_not_running() {
        #expect(status(coreFailed: true, hasSnapshot: true, modules: ["failed"], notice: true) == .notRunning)
    }

    @Test func no_snapshot_is_starting() {
        #expect(status(hasSnapshot: false, modules: []) == .starting)
    }
}

@Suite
struct LevelZeroTreeTests {
    private func ids(_ status: CoreStatus, dev: Bool) -> [String] {
        LevelZeroTree.rows(
            running: CascadeNode(id: "running", isActionable: true),
            core: CascadeNode(id: "core", isActionable: true),
            status: status,
            inDevProfile: dev
        ).map(\.id)
    }

    @Test func the_core_node_is_in_the_tree_only_where_the_row_is_shown() {
        let withCore = ["running", "core", "settings", "about", "quit"]
        let without = ["running", "settings", "about", "quit"]
        #expect(ids(.running, dev: false) == without)
        #expect(ids(.starting, dev: false) == without)
        #expect(ids(.needsAttention, dev: false) == withCore)
        #expect(ids(.notRunning, dev: false) == withCore)
        for status in [CoreStatus.starting, .running, .needsAttention, .notRunning] {
            #expect(ids(status, dev: true) == withCore)
        }
    }
}
