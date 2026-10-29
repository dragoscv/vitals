package app.vitals.core

import app.vitals.core.health.HealthClient
import app.vitals.core.health.HealthFailure
import app.vitals.core.health.HealthPoll
import app.vitals.core.health.HealthResult
import app.vitals.core.health.HealthUrl
import app.vitals.core.health.MeHealth
import app.vitals.core.health.PairState
import app.vitals.core.health.SchemePolicy
import kotlinx.coroutines.test.runTest
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class HealthClientTest {
    private val server = MockWebServer()
    private val token = "vmd_abcdefghijklmnopqrstuvwxyz012345"

    @Before
    fun start() = server.start()

    @After
    fun stop() = server.shutdown()

    // The scheme check is injected so a plain-http MockWebServer can stand in
    // for vmui; the production policy is tested separately below.
    private fun client(): HealthClient =
        requireNotNull(HealthClient.forBase("http://127.0.0.1:${server.port}", policy = { true }))

    private fun json(code: Int, body: String) =
        MockResponse().setResponseCode(code).setHeader("Content-Type", "application/json").setBody(body)

    @Test
    fun only_https_or_plain_http_to_this_phone_is_accepted_as_a_base_url() {
        assertEquals("https://vmui.example.com", HealthUrl.normalise("https://vmui.example.com/"))
        assertEquals("https://vmui.example.com/proxy", HealthUrl.normalise(" https://vmui.example.com/proxy/ "))
        assertEquals("http://localhost:3000", HealthUrl.normalise("http://localhost:3000"))
        assertEquals("http://127.0.0.1:3000", HealthUrl.normalise("http://127.0.0.1:3000/"))
        assertNull(HealthUrl.normalise("http://vmui.example.com"))
        assertNull(HealthUrl.normalise("http://192.168.1.10:3000"))
        assertNull(HealthUrl.normalise("ftp://vmui.example.com"))
        assertNull(HealthUrl.normalise("vmui.example.com"))
        assertNull(HealthUrl.normalise(""))
        assertNull(HealthUrl.normalise("https://user:pass@vmui.example.com"))
        assertNull(HealthUrl.normalise("https://vmui.example.com/?token=x"))
    }

    @Test
    fun a_client_is_refused_for_an_insecure_base_unless_the_policy_allows_it() {
        assertNull(HealthClient.forBase("http://vmui.example.com"))
        assertTrue(HealthClient.forBase("http://vmui.example.com", policy = SchemePolicy { true }) != null)
    }

    @Test
    fun a_full_body_decodes_every_reading_and_ignores_unknown_fields() = runTest {
        server.enqueue(json(200, FULL))
        val r = client().fetch(token)
        assertTrue("got $r", r is HealthResult.Ok)
        val h = (r as HealthResult.Ok).value
        assertEquals(1793200000000L, h.at)
        val readings = requireNotNull(h.health)
        assertEquals(8123L, readings.steps)
        assertEquals(6.1, requireNotNull(readings.distanceKm), 1e-9)
        assertEquals(72.0, requireNotNull(readings.heartRate), 1e-9)
        assertEquals(412.0, requireNotNull(readings.sleepMin), 1e-9)
        assertEquals(27.2, requireNotNull(readings.body?.bmi), 1e-9)
        assertEquals("over", readings.body.bmiBand)
        assertEquals(121.0, requireNotNull(readings.bloodPressure?.sys), 1e-9)
        assertEquals(2, readings.hrHistory.size)
        val n = requireNotNull(h.nutrition)
        assertEquals(2100.0, requireNotNull(n.targets?.calories), 1e-9)
        assertEquals(1460.0, requireNotNull(n.today?.calories), 1e-9)
        assertEquals(-350.0, requireNotNull(n.balance), 1e-9)
        assertEquals(2500.0, requireNotNull(n.water?.targetMl), 1e-9)
    }

    @Test
    fun a_sparse_body_keeps_every_missing_reading_null_never_zero() = runTest {
        server.enqueue(json(200, """{"at":1,"health":{"device":"p","steps":null,"heartRate":null},"nutrition":null}"""))
        val h = (client().fetch(token) as HealthResult.Ok).value
        val readings = requireNotNull(h.health)
        assertNull(readings.steps)
        assertNull(readings.heartRate)
        assertNull(readings.sleepMin)
        assertNull(readings.weightKg)
        assertNull(readings.body)
        assertNull(readings.bloodPressure)
        assertTrue(readings.hrHistory.isEmpty())
        assertNull(h.nutrition)
    }

    @Test
    fun a_body_with_health_null_decodes_as_no_phone_sensors() = runTest {
        server.enqueue(json(200, """{"at":5,"health":null,"nutrition":null}"""))
        val h = (client().fetch(token) as HealthResult.Ok).value
        assertNull(h.health)
        assertNull(h.nutrition)
    }

    @Test
    fun a_401_means_signed_out_and_a_403_means_not_the_owner() = runTest {
        server.enqueue(json(401, "{}"))
        server.enqueue(json(403, "{}"))
        server.enqueue(json(500, "{}"))
        assertEquals(HealthResult.Err(HealthFailure.SignedOut), client().fetch(token))
        assertEquals(HealthResult.Err(HealthFailure.NotOwner), client().fetch(token))
        val r = client().fetch(token)
        assertTrue("got $r", r is HealthResult.Err && r.failure is HealthFailure.Unreachable)
    }

    @Test
    fun an_unreadable_body_is_invalid_not_an_exception() = runTest {
        server.enqueue(json(200, "not json"))
        assertEquals(HealthResult.Err(HealthFailure.Invalid), client().fetch(token))
    }

    @Test
    fun the_token_travels_only_in_the_authorization_header_never_in_the_url() = runTest {
        server.enqueue(json(200, "{}"))
        client().fetch(token)
        val req = server.takeRequest()
        assertEquals("/api/v1/me/health", req.path)
        assertEquals("Bearer $token", req.getHeader("Authorization"))
        assertFalse(req.requestUrl.toString().contains(token))
    }

    @Test
    fun pairing_by_code_sends_the_android_platform_and_reads_the_code() = runTest {
        server.enqueue(json(200, """{"id":"0123456789abcdef","token":"$token","code":"1234","status":"pending"}"""))
        val r = client().pairStart("Pixel")
        assertEquals("1234", (r as HealthResult.Ok).value.code)
        val req = server.takeRequest()
        assertEquals("POST", req.method)
        assertEquals("/api/devices/pair", req.path)
        val body = req.body.readUtf8()
        assertTrue(body, body.contains("\"platform\":\"vitals-android\""))
        assertFalse(body, body.contains("password"))
    }

    @Test
    fun polling_a_pairing_maps_each_status_and_404_to_expired_and_429_to_slow_down() = runTest {
        server.enqueue(json(200, """{"status":"pending"}"""))
        server.enqueue(json(200, """{"status":"approved"}"""))
        server.enqueue(json(200, """{"status":"rejected"}"""))
        server.enqueue(json(404, "{}"))
        server.enqueue(json(429, "{}"))
        val c = client()
        assertEquals(HealthResult.Ok(PairState.Pending), c.pairPoll("abc"))
        assertEquals("/api/devices/pair?id=abc", server.takeRequest().path)
        assertEquals(HealthResult.Ok(PairState.Approved), c.pairPoll("abc"))
        assertEquals(HealthResult.Ok(PairState.Rejected), c.pairPoll("abc"))
        assertEquals(HealthResult.Ok(PairState.Expired), c.pairPoll("abc"))
        assertEquals(HealthResult.Ok(PairState.SlowDown), c.pairPoll("abc"))
    }

    @Test
    fun signing_in_with_wrong_credentials_is_signed_out_and_right_ones_return_the_token() = runTest {
        server.enqueue(json(401, "{}"))
        server.enqueue(json(200, """{"id":"x","token":"$token","status":"approved"}"""))
        assertEquals(HealthResult.Err(HealthFailure.SignedOut), client().signIn("Pixel", "a@b.c", "wrong"))
        assertEquals(HealthResult.Ok(token), client().signIn("Pixel", "a@b.c", "right"))
    }

    @Test
    fun only_the_first_eight_characters_of_a_token_are_ever_shown() {
        assertEquals("vmd_abcd", HealthClient.tokenPrefix(token))
    }

    @Test
    fun a_visible_page_polls_every_minute_after_a_good_answer() {
        val p = HealthPoll.after(HealthResult.Ok(MeHealth()), failures = 3)
        assertTrue(p is HealthPoll.Show)
        assertEquals(60_000L, p.delayMs)
    }

    @Test
    fun an_unreachable_vmui_keeps_the_last_data_and_backs_off_to_five_minutes() {
        val miss = HealthResult.Err(HealthFailure.Unreachable("x"))
        assertEquals(60_000L, HealthPoll.after(miss, 0).delayMs)
        assertEquals(120_000L, HealthPoll.after(miss, 1).delayMs)
        assertEquals(240_000L, HealthPoll.after(miss, 2).delayMs)
        assertEquals(300_000L, HealthPoll.after(miss, 3).delayMs)
        assertEquals(300_000L, HealthPoll.after(miss, 40).delayMs)
        assertTrue(HealthPoll.after(miss, 0) is HealthPoll.Stale)
    }

    @Test
    fun a_revoked_token_or_a_foreign_account_stops_polling() {
        assertTrue(HealthPoll.after(HealthResult.Err(HealthFailure.SignedOut), 0) is HealthPoll.Stop)
        assertTrue(HealthPoll.after(HealthResult.Err(HealthFailure.NotOwner), 0) is HealthPoll.Stop)
    }

    private companion object {
        val FULL = """
            {"at":1793200000000,"futureField":{"x":1},
             "health":{"device":"phone_slug","steps":8123,"stepsAt":1793199000000,"distanceKm":6.1,"floors":4,
               "activeKcal":410,"totalKcal":2310,"heartRate":72,"heartRateAt":1793199000000,"restingHr":58,
               "hrv":41,"spo2":97,"respiratoryRate":14,"sleepMin":412,"sleepAt":1793190000000,
               "weightKg":81.4,"weightAt":1793100000000,"bodyFat":18.2,"impedance":500,
               "body":{"bmi":27.2,"fatPct":21.0,"fatKg":17.1,"waterKg":46.0,"waterPct":56.5,"leanKg":64.3,
                 "muscleKg":35.0,"bmrKcal":1780,"boneKg":3.2,"ageYears":32,"bmiBand":"over"},
               "bmr":1790,"vo2max":44,"bloodPressure":{"sys":121,"dia":79},
               "hrHistory":[{"t":1793190000000,"v":71},{"t":1793191000000,"v":74}]},
             "nutrition":{"day":"2026-10-28",
               "targets":{"weightKg":81,"weightSource":"scale","ageYears":32,"bmr":1780,"tdee":2400,"calories":2100,
                 "protein":160,"carbs":220,"fats":70,"fiber":25},
               "today":{"day":"2026-10-28","calories":1460,"protein":98,"carbs":150,"fats":50,"fiber":18,"meals":3},
               "remaining":{"calories":640,"protein":62,"carbs":70,"fats":20},
               "balance":-350,"activeKcal":410,
               "water":{"day":"2026-10-28","ml":1500,"glasses":6,"targetMl":2500,"remainingMl":1000,
                 "lastAt":1793199000000,"underPace":true,"entries":[],"week":[{"day":"2026-10-27","ml":2100}]},
               "week":[{"day":"2026-10-27","calories":2050,"protein":150,"carbs":210,"fats":68,"fiber":22,"meals":4}]}}
        """.trimIndent()
    }
}
