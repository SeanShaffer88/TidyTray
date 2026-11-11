# Excluded features (Tier 3)

The following are **not implemented and will not be added**. They are listed
publicly so users can see the boundary, and so contributors know a pull
request adding one will be declined.

- RAM cleaners and "memory optimization" (including standby-list purges
  presented as optimization)
- Generic registry cleaners
- Windows Defender or Security Center removal or disabling
- SmartScreen disabling
- Permanent Windows Update disabling (pause/defer controls are allowed as
  Tier 2 actions)
- Bulk scheduled-task or service disabling ("disable everything")
- Driver removal
- Service disabling beyond the curated, individually documented Tier 2 list
- Any action that touches other user profiles automatically
- Telemetry-server blocking through HOSTS-file or firewall edits
  (deferred indefinitely: brittle, and hard to undo cleanly per entry)
- Any tweak or debloat action without a verified undo path
