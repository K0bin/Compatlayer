Simple Vulkan layer to lie about support for a few Vulkan features that Kosmickrisp doesn't support:

- Doom: Report support for depth bounds because the game needs it to start but can render just fine without it.
- DXVK D3D9 3+: Report support for fillModeNonSolid and geometry shaders because pretty much all D3D9 games run fine without them. Just don't play Sims 2.
