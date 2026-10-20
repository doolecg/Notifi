# GPU monitor

Shows GPU load, memory use and, where the hardware reports them, temperature and power.

## User-visible behaviour
- A GPU row/card in the monitor with a load percentage, video memory used/total and a small history graph.
- Temperature and power draw appear when the driver exposes them; missing values simply do not appear.
- With more than one GPU (integrated plus discrete) each is listed separately and the busiest can be the one shown in the tray. Sampling must not leak resources on multi-GPU machines.
- Per-app GPU use shows in the top-apps list when the OS reports it; grouped values are capped at 100%.
- Can be a tray readout (see tray-readouts.md) and an alert source for temperature (see alerts.md).
- Sampling runs only while something displays it (monitor window, tray readout, alert); it stops when closed.

## Settings
- Which GPU to follow (default: busiest)
- Show video memory (default on)
- Update interval (shared with system monitor)

## Default shortcuts
None. (Cmd maps to Ctrl, Option to Alt, Control to Ctrl/Win as fits.)

## Edge cases
- Driver does not report load (some older or virtual GPUs): row shows unavailable.
- GPU powered down (hybrid laptops): do not wake it just to read; show idle.
- Values over 100% or negative from the driver are clamped.
- NVIDIA tools missing: fall back to generic counters, no error popup.

## Permissions
None normally. Some Linux drivers restrict engine counters to root; the row then shows what is readable.

## Platform notes
- Windows: PDH "GPU Engine" and "GPU Adapter Memory" counters; NVML/ADL for temperature and power where installed.
- X11: amdgpu and i915 sysfs (gpu_busy_percent, hwmon); NVIDIA via NVML or nvidia-smi when present; Intel via intel_gpu_top-style counters if allowed.
- KDE Wayland: same as X11.
- GNOME Wayland: same as X11.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md
