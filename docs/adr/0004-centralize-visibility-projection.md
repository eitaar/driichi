# Centralize visibility projection

Every untrusted serializer consumes exactly one Player, Public, or Replay Admin projection and never canonical Match state. Human Players, Room MJAI, and MCP private state share Player visibility; Spectators and MCP public state share Public visibility; only the Admin's completed-Replay path is omniscient, and serialized leakage tests are a release gate.
