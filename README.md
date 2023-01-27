# Ard Engine

A game engine designed for open world 3D games.

## Goals

- **Moddable**: Users of games made with the engine will be able to easily modify, add, or remove content.
- **Streaming**: Streaming of assets will be handled automatically by the engine.
- **Big worlds**: The engine will support large open worlds.
- **Collaborative**: The editor will have integration with version control software, making collaboration easy.

## Features

### General

- High performance ECS.
- Event driven game loop.
- Plugin system.
- Async asset loading.

### Rendering

- 3D rendering.
- Bindless textures and materials.
- Unified vertex memory.
- GPU driven rendering.
- Texture streaming.
- Cascaded shadow mapping.
- Forward rendering with a Z-prepass.
- Clustered lighting.
- Hierarchical Z-buffer occlusion culling.
- PBR rendering with image based lighting.

## Directory Structure

- **crates**: Contains all the crates that make up the engine.
- **data**: Contains assets and configuration defaults for the editor.
- **src**: Contains the high-level crate for the entire engine which re-exports other crates.
- **tools**: Binaries used by the editor.

## Building

⚠️**WARNING**: The build files included are designed for a Windows environment. They will fail if
you attempt to build on another platform such as MacOS or Linux.

### Dependencies

Before following the build instructions, install the following dependencies.

| Dependency | Tested Version |
| - | - |
| [Rust](https://rustup.rs/) | 1.67.0 |
| [cargo-make](https://github.com/sagiegurari/cargo-make) | 0.36.4 |
| [Vulkan SDK](https://www.lunarg.com/vulkan-sdk/) | 1.3.236.0 |

### Instructions

1. Run `cargo make build_dbg` to compile the editor.
2. Run `/build/debug/ard-editor.exe` to use the editor.
