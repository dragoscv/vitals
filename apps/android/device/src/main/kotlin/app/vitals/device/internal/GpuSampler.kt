package app.vitals.device.internal

import app.vitals.device.model.GpuState

/**
 * GPU from the two sysfs shapes Samsung and Qualcomm expose. On the S25
 * Ultra `/sys/kernel/gpu` is readable while kgsl's `gpuclk` is denied, so the
 * Samsung node is tried first and each field falls back independently.
 */
internal class GpuSampler {
    private val present: Boolean by lazy {
        SysFs.read("$SAMSUNG/gpu_busy") != null || SysFs.read("$KGSL/gpubusy") != null
    }
    private val model: String? by lazy {
        (SysFs.read("$SAMSUNG/gpu_model") ?: SysFs.read("$KGSL/gpu_model"))?.trim()?.takeIf { it.isNotEmpty() }
    }
    private val maxHz: Long? by lazy {
        SysFs.readLong("$SAMSUNG/gpu_max_clock")?.let(::gpuClockHz) ?: SysFs.readLong("$KGSL/max_gpuclk")?.let(::gpuClockHz)
    }

    fun sample(): GpuState? {
        if (!present) return null
        val load = SysFs.read("$SAMSUNG/gpu_busy")?.let(::parseGpuBusy)
            ?: SysFs.read("$KGSL/gpubusy")?.let(::parseGpuBusy)
        val hz = SysFs.readLong("$SAMSUNG/gpu_clock")?.let(::gpuClockHz) ?: SysFs.readLong("$KGSL/gpuclk")?.let(::gpuClockHz)
        return GpuState(load = load, frequencyHz = hz, maxFrequencyHz = maxHz, model = model)
    }

    private companion object {
        const val SAMSUNG = "/sys/kernel/gpu"
        const val KGSL = "/sys/class/kgsl/kgsl-3d0"
    }
}
