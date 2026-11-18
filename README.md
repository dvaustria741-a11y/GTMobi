# GTMobi

Original open-world crime-sandbox for mobile, written in Rust. Visual target: GTA 6 reference
material (Vice City / Leonida look, warm dusk lighting, wetlands, neon). No Rockstar code or
assets are used.

Conventions: right-handed, +Y up, meters, radians, column-major matrices, clip depth 0..1.

## Status
Phase 1: workspace, `gtm-math`, `gtm-cull` (frustum + XZ grid + distance culling).
Phase 2a: `gtm-vehicle` (engine, gearbox, steering, suspension lean), ported from Defy (MIT), see THIRD_PARTY_NOTICES.md.
Phase 2b: `gtm-lighting` (time-of-day sun direction and keyframed dusk palette, uncalibrated art values).
Unit tests pass on host Linux (Rust 1.75); nothing built for Android or run on a device.

## Roadmap
1. Math + culling (done, unbuilt)
2. Windowing/input/Android glue (winit + android-activity), wgpu renderer bootstrap
3. Deferred-lite PBR pass, sky/atmosphere, dusk sun + point-light clustering, bloom, SSAO-lite
4. World streaming by county tile, LOD, instancing, occlusion (HZB)
5. Player controller, vehicles, traffic/pedestrian AI, police/wanted
6. UI (menu, settings, HUD, map) from custom art, see docs/ASSET_PROMPTS.md
7. Missions/save system, audio

## Build
    cargo test --workspace
