# Character Assets

Drop `.glb` files here. The renderer loads `player.glb` on startup if present.

## Free GTA 6-quality character sources

| Source | License | Format | Notes |
|--------|---------|--------|-------|
| [Ready Player Me](https://readyplayer.me) | Free (personal) | .glb | Realistic humanoids, skin tones, tattoos |
| [Mixamo](https://mixamo.com) | Free (personal) | FBX → convert with Blender → .glb | Rigged, 60+ animations |
| [Sketchfab CC0](https://sketchfab.com/search?licenses=7c23a1ba438d4306920229c12afcb5f9) | CC0 | .glb | Search "character CC0" |
| [Kenney](https://kenney.nl/assets/animated-characters-2) | CC0 | .glb | Stylized, good for NPCs |

## Export settings (Blender)
- Format: glTF 2.0 (.glb)
- Include: Mesh data, Materials, Textures (embedded), Normals, Tangents, UVs
- Transform: Y-up (default)
- Compression: off (Draco not supported by our gltf crate yet)

## Naming convention
```
player.glb       — main playable character
npc_male.glb     — generic male NPC pool
npc_female.glb   — generic female NPC pool
```
