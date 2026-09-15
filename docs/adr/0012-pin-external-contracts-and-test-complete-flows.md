# Pin external contracts and test complete flows

Formal Design Freeze depends on exact revisions of `riichienv-core`, riichi.dev Protocol v2, `yamai`, `rmcp`, and tile assets, with pinned local fixtures and executable compatibility probes as the release contract. CI validates complete three- and four-player Human Matches, serialized visibility, real `yamai ReplayProcessor` consumption, MCP session behavior, platform smoke tests, and the same production Bot against local fixtures; scheduled live riichi.dev checks report drift without redefining the pin.
