/// Display names and marks for a Harness id. Must match `navori_protocol::Harness`.
public enum HarnessCopy {
    public static func display(_ id: String) -> String? {
        switch id {
        case "claude", "claude_code", "Claude Code": "Claude Code"
        case "codex", "Codex": "Codex"
        case "grok", "Grok", "Grok Build": "Grok Build"
        case "": nil
        default: id
        }
    }

    public static func mark(_ id: String?) -> HoldingMark {
        switch id {
        case "claude", "claude_code", "Claude Code": .claude
        case "codex", "Codex": .codex
        case "grok", "Grok", "Grok Build": .grok
        default: .terminal
        }
    }
}
