import Foundation

/// Links `~/.local/bin/navori` to the CLI inside the installed bundle (release profile only;
/// dev builds print a command that passes `--profile dev`).
enum CLIInstaller {
    static var linkURL: URL {
        FileManager.default.homeDirectoryForCurrentUser.appending(path: ".local/bin/navori")
    }

    static var isInstalled: Bool {
        guard let dest = try? FileManager.default.destinationOfSymbolicLink(atPath: linkURL.path) else { return false }
        return dest == AppEnvironment.cliURL.path
    }

    /// The command to type in a terminal for this build.
    static var command: String {
        AppEnvironment.isDev ? "\"\(AppEnvironment.cliURL.path)\" --profile dev" : "navori"
    }

    static func install() throws {
        let fm = FileManager.default
        try fm.createDirectory(at: linkURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        if (try? fm.destinationOfSymbolicLink(atPath: linkURL.path)) != nil || fm.fileExists(atPath: linkURL.path) {
            try fm.removeItem(at: linkURL)
        }
        try fm.createSymbolicLink(at: linkURL, withDestinationURL: AppEnvironment.cliURL)
    }
}
