# 0005 — Navori may reach external services through accounts the user connects

Status: Accepted · 2026-11-02

Limiting network calls to GitHub and the configured Providers kept the attack surface small but stops Navori from being the user's assistant across mail and chat, so we widen the limit to services the user explicitly connects, as a Connected account or an MCP server, still with no Navori-owned account or server. This lets content written by strangers reach Navori AI and lets it speak as the user, so incoming content is untrusted and sending to another person is `Destructive` (THR-11, THR-12 in `docs/security/threat-model.md`). Each service goes through the MCP client first, and no shared framework for Connected accounts is built until a second native service shows what it must hold.
