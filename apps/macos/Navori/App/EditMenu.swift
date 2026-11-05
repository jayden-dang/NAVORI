import AppKit

/// A menubar-only (accessory) app has no main menu, so AppKit has no Edit menu to route
/// ⌘V, ⌘C, ⌘X, ⌘A and ⌘Z through, and a text field in Settings cannot take a paste.
/// The menu bar stays hidden for an accessory app; the items only supply the key equivalents.
@MainActor
enum EditMenu {
    static func install(on app: NSApplication = .shared) {
        let main = NSMenu()
        let editItem = NSMenuItem()
        main.addItem(editItem)

        let edit = NSMenu(title: "Edit")
        edit.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
        let redo = edit.addItem(withTitle: "Redo", action: Selector(("redo:")), keyEquivalent: "z")
        redo.keyEquivalentModifierMask = [.command, .shift]
        edit.addItem(.separator())
        edit.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        edit.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        edit.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        edit.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        editItem.submenu = edit

        app.mainMenu = main
    }
}
