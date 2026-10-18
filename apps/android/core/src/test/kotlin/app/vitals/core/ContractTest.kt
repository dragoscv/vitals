package app.vitals.core

import app.vitals.core.model.Alert
import app.vitals.core.model.Frame
import app.vitals.core.model.FramePayload
import app.vitals.core.model.HostInfo
import app.vitals.core.model.MachineSample
import app.vitals.core.model.ProcessState
import app.vitals.core.model.SensorLine
import app.vitals.core.model.Summary
import app.vitals.core.model.ThrottleReason
import app.vitals.core.model.activeSessions
import kotlinx.serialization.KSerializer
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Decodes the JSON that `crates/vitals-server/tests/android_contract.rs`
 * serialised from real Rust values.
 *
 * Strict: an unknown key fails, and a round trip must reproduce every key
 * the Rust side wrote. Either direction of drift — Rust adds a field the
 * model lacks, or the model expects one Rust renamed — goes red here.
 */
class ContractTest {
    private fun text(name: String): String =
        requireNotNull(javaClass.getResource("/contract/$name")) { "missing fixture $name; run cargo test -p vitals-server --test android_contract" }
            .readText()

    private fun <T> roundTrip(name: String, serializer: KSerializer<T>): T {
        val raw = text(name)
        val value = WireJson.Strict.decodeFromString(serializer, raw)
        val original = WireJson.Strict.parseToJsonElement(raw)
        val again = WireJson.Strict.parseToJsonElement(WireJson.Strict.encodeToString(serializer, value))
        assertSameKeys(name, original, again)
        return value
    }

    /** Every object key present on one side is present on the other, at every depth. */
    private fun assertSameKeys(path: String, a: JsonElement, b: JsonElement) {
        if (a is JsonObject && b is JsonObject) {
            assertEquals("keys at $path", a.keys, b.keys)
            a.keys.forEach { assertSameKeys("$path.$it", a.getValue(it), b.getValue(it)) }
        } else if (a is kotlinx.serialization.json.JsonArray && b is kotlinx.serialization.json.JsonArray) {
            assertEquals("length at $path", a.size, b.size)
            a.indices.forEach { assertSameKeys("$path[$it]", a[it], b[it]) }
        }
    }

    @Test
    fun a_full_keyframe_decodes_with_every_optional_measured() {
        val frame = roundTrip("keyframe_full.json", Frame.serializer())
        val p = frame.payload as FramePayload.Keyframe
        assertEquals(81.5f, p.system.cpu.temperature)
        assertEquals(ThrottleReason.PowerLimit, p.system.cpu.throttled)
        assertEquals(16f, p.system.gpus.single().utilization)
        assertEquals("AA-BB-CC-DD-EE-FF", p.system.networks.single().mac)
        assertEquals(ProcessState.NotResponding, p.processes.single().state)
        // FILETIME-sized start times must survive as Long, not be rounded.
        assertEquals(133_700_000_000_000_000L, p.processes.single().key.startTime)
    }

    @Test
    fun a_sparse_keyframe_keeps_unmeasured_values_null_not_zero() {
        val frame = roundTrip("keyframe_sparse.json", Frame.serializer())
        val cpu = frame.payload.system.cpu
        assertNull(cpu.temperature)
        assertNull(cpu.power)
        assertNull(frame.payload.system.battery)
    }

    @Test
    fun a_delta_carries_exits_and_changes() {
        val frame = roundTrip("delta.json", Frame.serializer())
        val d = frame.payload as FramePayload.Delta
        assertEquals(listOf(7), d.exited)
        assertEquals(1, d.changed.size)
    }

    @Test
    fun summary_history_sensors_alerts_and_host_decode() {
        val s = roundTrip("summary.json", Summary.serializer())
        assertEquals(1, s.processCount)
        val h = roundTrip("history.json", ListSerializer(MachineSample.serializer()))
        assertNotNull(h.first().cpuTempC)
        assertNull(h.last().cpuTempC)
        val sensors = roundTrip("sensors.json", ListSerializer(SensorLine.serializer()))
        assertEquals("temperature", sensors.single().unit)
        val alerts = roundTrip("alerts.json", ListSerializer(Alert.serializer()))
        assertTrue(alerts.single().values.containsKey("percent"))
        val host = roundTrip("host.json", HostInfo.serializer())
        assertEquals(32, host.logicalCores)
    }

    @Test
    fun the_agents_card_reads_what_the_pc_serves() {
        // Lenient, not roundTrip: the card mirrors only the fields it shows
        // (Agents.kt explains why). A renamed field it does read still fails.
        val snap = WireJson.Lenient.decodeFromString(
            app.vitals.core.model.AgentsSnapshot.serializer(),
            text("agents.json"),
        )
        assertTrue(snap.agentqInstalled)
        val queue = snap.resources.single()
        assertEquals("build:codai", queue.name)
        assertEquals(1, queue.holding)
        assertEquals(1, queue.waiting)
        val session = snap.sessions.single()
        assertEquals(app.vitals.core.model.AgentClient.VscodeCopilot, session.client)
        assertEquals(app.vitals.core.model.SessionState.Working, session.state)
        assertEquals("run_in_terminal", session.currentTool)
        assertEquals(listOf(session), snap.activeSessions)
    }

    @Test
    fun agent_enums_a_newer_desktop_added_decode_to_unknown_instead_of_hiding_the_card() {
        // AND2-1: with generated serializers one new client, state or
        // liveness failed the whole answer and the card vanished.
        val raw = text("agents.json")
            .replace("\"vscodeCopilot\"", "\"cursorAgent\"")
            .replace("\"working\"", "\"thinking\"")
            .replace("\"alive\"", "\"zombie\"")
        val snap = WireJson.Lenient.decodeFromString(app.vitals.core.model.AgentsSnapshot.serializer(), raw)
        val session = snap.sessions.single()
        assertEquals(app.vitals.core.model.AgentClient.Unknown, session.client)
        assertEquals(app.vitals.core.model.SessionState.Unknown, session.state)
        assertTrue(snap.resources.single().tickets.all { it.liveness == app.vitals.core.model.Liveness.Unknown })
        // An unknown state is never counted as someone working.
        assertTrue(snap.activeSessions.isEmpty())
    }

    @Test
    fun the_allowlisted_answer_without_any_free_text_still_decodes() {
        // The PC's sharing allowlist sends no title, command, purpose, host or
        // repo. Every one of them must be optional, or the card disappears the
        // day the desktop stops sending it.
        val raw = """{"takenAtMs":1,"agentqInstalled":true,""" +
            """"resources":[{"name":"build:x","tickets":[{"held":true,"liveness":"alive"}]}],""" +
            """"sessions":[{"id":"s","client":"codex","state":"waiting"}]}"""
        val snap = WireJson.Lenient.decodeFromString(app.vitals.core.model.AgentsSnapshot.serializer(), raw)
        val session = snap.sessions.single()
        assertNull(session.title)
        assertNull(session.repo)
        assertNull(session.host)
        assertNull(snap.resources.single().tickets.single().purpose)
        assertEquals(listOf(session), snap.activeSessions)
        // And an empty object is an empty answer, not a failure.
        val empty = WireJson.Lenient.decodeFromString(app.vitals.core.model.AgentsSnapshot.serializer(), "{}")
        assertTrue(empty.sessions.isEmpty() && empty.resources.isEmpty() && !empty.agentqInstalled)
    }
}
