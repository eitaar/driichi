# Task 14 foundation brief

Base: `e2b1b79` (`docs: record Task 13 provisional acceptance`).

Added compact unit coverage for the authenticated MCP foundation: exact four-tool/three-template surface, permanent first-room binding, token-room resume and replacement generations, idle TTL disconnect, revision wake filtering and lost-wakeup ordering, stable action error codes, private/public/history redaction, and exposed Room leave/disconnect, revocation, deletion, and shutdown transitions. Projection assertions use the touched `RoomHandle::public_projection` seam.

Live HTTP/bridge coverage is intentionally deferred to the next phase. The existing foundation changes and this artifact are committed together as `feat(mcp): add authenticated session resources`.
