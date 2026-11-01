import Foundation

/// Facts about this build, read from Info.plist (written by `cargo xtask bundle`).
enum AppEnvironment {
    /// `"release"` or `"dev"`. Dev builds use separate data, logs and socket.
    static let profile: String = info("NavoriProfile") ?? "dev"
    static let version: String = info("CFBundleShortVersionString") ?? "0.0.0"
    static let build: String = info("CFBundleVersion") ?? "0"
    static let bundleID: String = Bundle.main.bundleIdentifier ?? "dev.navori.app.debug"

    static var isDev: Bool { profile != "release" }
    static var displayName: String { isDev ? "Navori Dev" : "Navori" }

    private static let perfFlag = ProcessInfo.processInfo.environment["NAVORI_PERF"] == "1"

    static let perfHome: String? = {
        guard let home = ProcessInfo.processInfo.environment["NAVORI_PERF_HOME"], !home.isEmpty else {
            return nil
        }
        return home
    }()

    static let perfMode = perfFlag && perfHome != nil

    static let coreHome: String = {
        if perfMode, let perfHome { return perfHome }
        return NSHomeDirectory()
    }()

    static let perfFlagWithoutHome = perfFlag && perfHome == nil

    /// The `navori` CLI shipped inside the bundle.
    static var cliURL: URL {
        Bundle.main.bundleURL.appending(path: "Contents/Helpers/navori")
    }

    private static func info(_ key: String) -> String? {
        Bundle.main.object(forInfoDictionaryKey: key) as? String
    }
}
