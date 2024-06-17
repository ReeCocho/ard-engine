# Ard Engine

A game engine designed for open world 3D games.

## Goals

- **Moddable**: Users of games made with the engine will be able to easily modify, add, or remove content.
- **Streaming**: Streaming of assets will be handled automatically by the engine.
- **Big worlds**: The engine will support large open worlds.
- **Collaborative**: The editor will have integration with version control software, making collaboration easy.

## Features

### General

- High performance parallelized ECS.
- Event driven game loop.
- Plugin system.
- Async asset loading.

### Rendering

⚠️**WARNING**: In order to use the renderer, you need a GPU with support for Vulkan 1.3, mesh shading, and ray tracing. I do not have plans to support GPUs without these features.

- 3D rendering.
- Bindless textures and materials.
- Unified vertex memory.
- GPU driven rendering.
- Texture streaming.
- Cascaded shadow mapping.
- Forward rendering with a Z-prepass.
- Clustered lighting.
- GPU occlusion culling.
- PBR rendering with realtime image based lighting.
- Effects like ambient occlusion, crepuscular rays, bloom, and more.
- Mesh shading.
- Hardware accelerated path tracing reference.

## Directory Structure

- **crates**: Contains all the crates that make up the engine.
- **src**: Contains the high-level crate for the entire engine which re-exports other crates.
- **tools**: Binaries used by the editor.

## Building

⚠️**WARNING**: The build files are tested in a Windows environment. They might fail if
you attempt to build on another platform. If you encounter any problems, please feel free to open an issue ticket.

### Dependencies

Before following the build instructions, install the following dependencies.

| Dependency | Tested Version |
| - | - |
| [Rust](https://rustup.rs/) | 1.79.0 |
| [cargo-make](https://github.com/sagiegurari/cargo-make) | 0.37.12 |
| [Vulkan SDK](https://www.lunarg.com/vulkan-sdk/) | 1.3.283.0 |

### Instructions

1. Run `cargo make --profile=opt-dev build-editor` to compile the editor.
2. Run `/build/opt-dev/ard-editor.exe` to use the editor.
