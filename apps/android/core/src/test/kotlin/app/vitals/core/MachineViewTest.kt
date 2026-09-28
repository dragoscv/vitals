package app.vitals.core

import app.vitals.core.model.Frame
import app.vitals.core.model.FramePayload
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class MachineViewTest {
    private val keyframe = WireJson.Strict.decodeFromString(
        Frame.serializer(),
        javaClass.getResource("/contract/keyframe_full.json")!!.readText(),
    )
    private val proc = (keyframe.payload as FramePayload.Keyframe).processes.single()

    private fun delta(changed: List<app.vitals.core.model.Process>, exited: List<Int>) =
        keyframe.copy(seq = 9, payload = FramePayload.Delta(keyframe.payload.system, changed, exited))

    @Test
    fun a_delta_before_any_keyframe_is_dropped_rather_than_invented() {
        assertNull(MachineView.fold(null, delta(listOf(proc), emptyList())))
    }

    @Test
    fun exits_apply_before_changes_so_a_recycled_pid_survives() {
        val start = MachineView.of(keyframe)!!
        val reborn = proc.copy(name = "new.exe", key = proc.key.copy(startTime = 1))
        val next = start.apply(delta(listOf(reborn), listOf(proc.key.pid)))
        assertEquals("new.exe", next.processes.getValue(proc.key.pid).name)
        assertEquals(9, next.seq)
    }

    @Test
    fun an_exit_removes_the_process() {
        val next = MachineView.of(keyframe)!!.apply(delta(emptyList(), listOf(proc.key.pid)))
        assertEquals(0, next.processes.size)
    }
}
