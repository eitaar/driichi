# Use broadcast-noir UI with an accessibility baseline

The original broadcast-noir/Pixi visual decision is superseded for Live Match and Replay by ADR 0014 and the human-approved soft-table integration in `DESIGN.md`. React Three Fiber owns the visual table; React DOM owns controls, status and the local concealed hand in both surfaces. Semantic DOM controls, usable contrast, visible keyboard focus and reduced-motion fallbacks remain required. Mobile gameplay layout remains deferred. The supersession does not imply integrated visual, browser or GPU acceptance.
