# Use Decisions and complete actions

The Match machine opens versioned Decisions containing eligible Players, complete legal Game Actions, a deterministic timeout default, and server-monotonic timing. Human and MCP clients submit ephemeral action IDs, MJAI actions map through its compatibility adapter, simultaneous responses resolve only after every eligible response or timeout, and stale submissions or impossible defaults cannot mutate state.
