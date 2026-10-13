# 0039. Hide unused discrete-GPU drivers on hybrid Linux laptops

- Status: accepted
- Date: 2026-10-13
- Amends: [0027](0027-performance-budgets.md) (how the startup and memory budgets are met)

## Context
GPUI's wgpu renderer (`gpui-pre-wgpu` 0.3.8) enumerates every Vulkan and GL adapter before choosing one,
and it ranks the GPU the compositor runs on first. On the reference laptop, the display runs on an AMD iGPU
and there is also an NVIDIA dGPU. Enumeration loads NVIDIA's proprietary Vulkan, GL and EGL drivers anyway.
They are never used, but they cost:

| (release build, fresh data dir) | first frame | idle RSS |
|---|---|---|
| all drivers (GPUI default) | 275–306 ms | 178–184 MB |
| NVIDIA Vulkan and EGL hidden | 184–242 ms | 108–111 MB |
| also no GL backend at all | 173–185 ms | 86 MB |

Loading the NVIDIA driver also wakes a runtime-suspended dGPU, which added about a second to cold start
when measured on 2026-10-11. Budgets (ADR-0027): first frame under 300 ms, idle memory under 150 MB.

The backend set is hard-coded in GPUI, and neither GPUI nor wgpu reads a variable that restricts it. The
Vulkan loader (`VK_LOADER_DRIVERS_DISABLE`) and glvnd (`__EGL_VENDOR_LIBRARY_FILENAMES`) can hide drivers.

## Decision
At the top of `main`, on Linux, before any thread starts, `gremliner::gpu::select_drivers` runs these steps:
1. Read `/sys/class/drm/card*/device/{vendor,boot_vga}`.
2. Act only when the boot (display) GPU is not NVIDIA and an NVIDIA GPU is present.
3. Then set `VK_LOADER_DRIVERS_DISABLE=*nvidia*`, which matches `nvidia_icd.json` only, so nouveau is
   unaffected.
4. Also set `__EGL_VENDOR_LIBRARY_FILENAMES` to Mesa's glvnd vendor file, when one exists.

Nothing changes when:
- the user sets `GREMLINER_GPU=all`;
- the user sets any of these themselves: `ZED_DEVICE_ID`, `VK_LOADER_DRIVERS_DISABLE` or `_SELECT`,
  `VK_ICD_FILENAMES`, `VK_DRIVER_FILES`, `__EGL_VENDOR_LIBRARY_*`;
- the display GPU is NVIDIA;
- there is no NVIDIA GPU.

Setting the environment needs one justified `unsafe` block (`std::env::set_var`). It is sound because no
other thread exists yet.

We keep Mesa's GL backend. It is the fallback when Vulkan cannot configure the surface, and dropping it
would need a patched GPUI or a relaunch on failure.

## Alternatives considered
- **Patch `gpui-pre-wgpu`** to enumerate only the compositor's GPU, or Vulkan before GL. This saves the
  most (the third row), but means a vendored fork of a fast-moving dependency.
- **Do nothing.** The startup budget then fails intermittently, the memory budget fails consistently, and
  a suspended dGPU wakes on every launch, costing battery.
- **Relaunch with a reduced environment** instead of `set_var`. No `unsafe`, but it adds a process start.

## Consequences
- On hybrid laptops the app never touches the dGPU. Users who want it set `GREMLINER_GPU=all` and choose it
  with `ZED_DEVICE_ID`.
- NVIDIA-primary desktops and single-GPU machines behave as before. Their startup and memory depend on what
  GPUI's full enumeration loads there.

## Revisit when
GPUI restricts or lazily enumerates adapters itself. Then remove `gpu.rs`.
