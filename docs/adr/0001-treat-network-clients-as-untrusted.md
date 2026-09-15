# Treat network clients as untrusted

Although Double Riichi is self-hosted and binds to localhost by default, it may be exposed through a reverse proxy, so every browser, MJAI, and MCP client is untrusted. Browser APIs remain same-origin, Human and Admin credentials use HttpOnly cookies, Bots use Bearer headers, forwarded IP data is accepted only from configured proxies, and authorization, rate limits, path safety, security headers, and bounded message sizes are enforced by the server.
