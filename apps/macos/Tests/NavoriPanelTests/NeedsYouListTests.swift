import CoreGraphics
import NavoriPanel
import Testing

@Suite
struct NeedsYouListTests {
    @Test func testFilterTabs() {
        let todos = ["t1", "t2"]
        let updates = ["u1", "u2"]

        let all = NeedsYouList.filter(.all, todos: todos, updates: updates)
        #expect(all.todos == todos)
        #expect(all.updates == updates)

        let onlyTodos = NeedsYouList.filter(.todos, todos: todos, updates: updates)
        #expect(onlyTodos.todos == todos)
        #expect(onlyTodos.updates.isEmpty)

        let onlyUpdates = NeedsYouList.filter(.updates, todos: todos, updates: updates)
        #expect(onlyUpdates.todos.isEmpty)
        #expect(onlyUpdates.updates == updates)
    }

    @Test func testDrawnIdsAreOnlyRowsInsideTheViewport() {
        let viewport = CGRect(x: 0, y: 0, width: 350, height: 100)
        let frames: [Int64: CGRect] = [
            1: CGRect(x: 0, y: 10, width: 350, height: 26),
            2: CGRect(x: 0, y: 40, width: 350, height: 26),
            3: CGRect(x: 0, y: 120, width: 350, height: 26),
            4: CGRect(x: 0, y: -30, width: 350, height: 26),
        ]
        #expect(NeedsYouList.drawnIds(rowFrames: frames, viewport: viewport) == [1, 2])
    }

    @Test func retry_on_a_failed_secondary_choice_sends_the_secondary_request() {
        #expect(
            NeedsYouList.answerRequest(
                primary: true,
                failedChoice: "secondary",
                primaryRequest: "primary-json",
                secondaryRequest: "secondary-json"
            ) == "secondary-json"
        )
        #expect(
            NeedsYouList.answerRequest(
                primary: true,
                failedChoice: "primary",
                primaryRequest: "primary-json",
                secondaryRequest: "secondary-json"
            ) == "primary-json"
        )
        #expect(
            NeedsYouList.answerRequest(
                primary: true,
                failedChoice: nil,
                primaryRequest: "primary-json",
                secondaryRequest: "secondary-json"
            ) == "primary-json"
        )
        #expect(
            NeedsYouList.answerRequest(
                primary: false,
                failedChoice: "secondary",
                primaryRequest: "primary-json",
                secondaryRequest: "secondary-json"
            ) == "secondary-json"
        )
    }

    @Test func testDecideKeys() {
        #expect(DecideKey.from(keyCode: 36, modifiers: []) == .primary)
        #expect(DecideKey.from(keyCode: 76, modifiers: []) == .primary)
        #expect(DecideKey.from(keyCode: 51, modifiers: [.command]) == .secondary)
        #expect(DecideKey.from(keyCode: 51, modifiers: []) == nil)
        #expect(DecideKey.from(keyCode: 36, modifiers: [.command]) == nil)
        #expect(DecideKey.from(keyCode: 0, modifiers: []) == nil)
    }
}
