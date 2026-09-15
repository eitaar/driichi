# Use Resource-centric MCP with a revision wait

MCP v1 uses Streamable HTTP with private pre-shared Bot Bearer authentication rather than OAuth, revalidates every request independently of the transport session ID, and binds one Token-owned MCP Participant per Room. State remains available only through revisioned private, public, and history Resources, while a synchronization-only `wait_for_turn(after_revision)` Tool prevents lost wakeups on hosts that do not invoke an LLM from Resource notifications.
