package app.vitals.wear.screens

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasScrollAction
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeUp
import androidx.compose.ui.text.TextLayoutResult
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.wear.compose.material3.AppScaffold
import androidx.wear.compose.material3.TimeText
import app.vitals.core.model.Process
import app.vitals.wear.data.ControlOutcome
import app.vitals.wear.ui.LicencesList
import app.vitals.wear.ui.PcDetailScreen
import app.vitals.wear.ui.PcListScreen
import app.vitals.wear.ui.ProcessesScreen
import app.vitals.wear.ui.SensorsScreen
import app.vitals.wear.ui.VitalsTheme
import app.vitals.wear.ui.WatchSelfList
import com.github.takahirom.roborazzi.RoborazziOptions
import com.github.takahirom.roborazzi.captureRoboImage
import com.mikepenz.aboutlibraries.entity.Library
import com.mikepenz.aboutlibraries.entity.License
import kotlin.math.hypot
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * Play "Wear App Quality: Watch shapes" guard (S22-12; 1.0.4 rejected on
 * 2026-10-19 because the home card, the empty-state text and the sensor rows
 * were cut by a round bezel).
 *
 * Every screen of the watch app is rendered inside the real AppScaffold and
 * TimeText, and checked at rest at the top of its list and again at the end:
 *
 * - each line of text (from its own text layout, so a centred short line is
 *   judged by its ink, not by its box) must have all four corners inside the
 *   display circle;
 * - each control is treated as the pill or circle Wear draws, and its ends
 *   must lie inside the circle.
 *
 * On a scrolling screen the first items rest in the upper half and the last
 * in the lower half, so each state checks its own half; items beyond the
 * fold on the other side are the scroll affordance. Material 3 list items
 * morph narrower inside the transition areas (about the outer 35 % at each
 * edge), so controls are judged in the band between them, where they are
 * drawn at their layout width. Square watches are checked against the
 * rectangle instead. Each final state is saved under
 * `src/test/snapshots/<device>/` for review.
 */
@RunWith(AndroidJUnit4::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
abstract class WatchShapeTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    protected abstract val device: String
    protected open val round: Boolean = true

    private fun check(name: String, content: @Composable () -> Unit) {
        compose.setContent {
            VitalsTheme { AppScaffold(timeText = { TimeText() }) { content() } }
        }
        compose.waitForIdle()
        val problems = mutableListOf<String>()
        val list = compose.onAllNodes(hasScrollAction()).fetchSemanticsNodes().isNotEmpty()
        problems += violations("$name/top", list, Half.Upper)
        if (list) {
            // The top of a list is what a person sees first, and what Play's
            // reviewer photographed; keep it as its own baseline.
            compose.onRoot().captureRoboImage("src/test/snapshots/$device/${name}_top.png", OPTIONS)
            repeat(SWIPES) {
                compose.onAllNodes(hasScrollAction())[0].performTouchInput { swipeUp() }
                compose.waitForIdle()
            }
            problems += violations("$name/end", list, Half.Lower)
        }
        compose.onRoot().captureRoboImage("src/test/snapshots/$device/$name.png", OPTIONS)
        assertTrue(problems.joinToString("\n"), problems.isEmpty())
    }

    private enum class Half { Upper, Lower }

    private fun violations(where: String, list: Boolean, half: Half): List<String> {
        val root = compose.onRoot().fetchSemanticsNode().boundsInRoot
        val r = minOf(root.width, root.height) / 2f
        val c = root.center
        val band = if (list) (root.top + root.height * MORPH)..(root.bottom - root.height * MORPH) else root.top..root.bottom
        fun inside(p: Offset): Boolean =
            if (round) {
                hypot(p.x - c.x, p.y - c.y) <= r + TOLERANCE
            } else {
                p.x >= root.left - TOLERANCE && p.x <= root.right + TOLERANCE && p.y >= root.top - TOLERANCE && p.y <= root.bottom + TOLERANCE
            }
        val out = mutableListOf<String>()
        for (node in compose.onAllNodes(HAS_TEXT, useUnmergedTree = true).fetchSemanticsNodes()) {
            val layout = textLayout(node) ?: continue
            val origin = node.positionInRoot
            for (i in 0 until layout.lineCount) {
                val line = Rect(
                    origin.x + layout.getLineLeft(i),
                    origin.y + layout.getLineTop(i),
                    origin.x + layout.getLineRight(i),
                    origin.y + layout.getLineBottom(i),
                )
                if (line.width < 1f) continue
                // Lines entirely off screen are not drawn.
                if (line.bottom <= root.top || line.top >= root.bottom) continue
                if (list && half == Half.Upper && line.top > c.y) continue
                if (list && half == Half.Lower && line.bottom < c.y) continue
                if (isTimeText(node)) continue
                val corners = listOf(line.topLeft, line.topRight, line.bottomLeft, line.bottomRight)
                if (!corners.all(::inside)) out += "$where text '${lineText(node, layout, i)}' at $line"
            }
        }
        for (node in compose.onAllNodes(CLICKABLE).fetchSemanticsNodes()) {
            val b = node.boundsInRoot
            if (b.width >= root.width * FULL || b.height >= root.height * FULL) continue
            if (b.width < 1f || b.height < 1f || b.top < band.start - TOLERANCE || b.bottom > band.endInclusive + TOLERANCE) continue
            val rad = minOf(b.width, b.height) / 2f
            val ends = if (b.width >= b.height) {
                listOf(Offset(b.left + rad, b.center.y), Offset(b.right - rad, b.center.y))
            } else {
                listOf(Offset(b.center.x, b.top + rad), Offset(b.center.x, b.bottom - rad))
            }
            val fits = if (round) ends.all { hypot(it.x - c.x, it.y - c.y) + rad <= r + TOLERANCE } else ends.all(::inside)
            if (!fits) out += "$where control '${label(node)}' at $b"
        }
        return out
    }

    /** The curved clock draws along the bezel by design; it is not list content. */
    private fun isTimeText(node: SemanticsNode): Boolean {
        val text = node.config.getOrNull(SemanticsProperties.Text)?.joinToString { it.text }.orEmpty()
        return TIME.matches(text)
    }

    private fun textLayout(node: SemanticsNode): TextLayoutResult? {
        val results = mutableListOf<TextLayoutResult>()
        node.config.getOrNull(SemanticsActions.GetTextLayoutResult)?.action?.invoke(results)
        return results.firstOrNull()
    }

    private fun lineText(node: SemanticsNode, layout: TextLayoutResult, i: Int): String {
        val text = layout.layoutInput.text.text
        return text.substring(layout.getLineStart(i), layout.getLineEnd(i)).ifBlank { label(node) }
    }

    private fun label(node: SemanticsNode): String =
        node.config.getOrNull(SemanticsProperties.Text)?.joinToString { it.text }
            ?: node.config.getOrNull(SemanticsProperties.ContentDescription)?.joinToString()
            ?: "#${node.id}"

    private val monitor = FakeMonitor(Fixtures.snapshot)

    @Test
    fun home_with_no_pcs() = check("home_empty") {
        PcListScreen(emptyList(), emptyMap(), loaded = true, onOpen = {}, device = monitor, onOpenSelf = {}, onOpenLicences = {})
    }

    @Test
    fun home_with_pcs() = check("home_pcs") {
        val other = Fixtures.pairing.copy(id = "p2", label = "Laptop")
        PcListScreen(
            listOf(Fixtures.pairing, other),
            mapOf(Fixtures.pairing.id to Fixtures.reachable, other.id to Fixtures.unreachable.copy(pairingId = other.id)),
            loaded = true, onOpen = {}, device = monitor, onOpenSelf = {}, onOpenLicences = {},
        )
    }

    @Test
    fun watch_self() = check("watch_self") { WatchSelfList(Fixtures.snapshot, Fixtures.storage, Fixtures.hardwareSensors) }

    @Test
    fun pc_detail() = check("pc_detail") { PcDetailScreen(Fixtures.reachable, Fixtures.pairing.label, onSensors = {}, onProcesses = {}) }

    @Test
    fun pc_detail_unreachable() = check("pc_detail_unreachable") {
        PcDetailScreen(Fixtures.unreachable, Fixtures.pairing.label, onSensors = {}, onProcesses = {})
    }

    @Test
    fun sensors() = check("sensors") { SensorsScreen(Fixtures.sensors, reachable = true) }

    @Test
    fun sensors_empty() = check("sensors_empty") { SensorsScreen(emptyList(), reachable = false) }

    @Test
    fun processes() = check("processes") {
        val many: List<Process> = List(6) { i ->
            Fixtures.summary.top.first().let { p -> p.copy(key = p.key.copy(pid = p.key.pid + i), description = "Google Chrome renderer helper process #$i") }
        }
        ProcessesScreen(many, canControl = false, onEndTask = { ControlOutcome.Done })
    }

    @Test
    fun licences() = check("licences") {
        val libs = List(6) { i ->
            Library(
                uniqueId = "lib$i",
                artifactVersion = "1.7.$i",
                name = "androidx.wear.compose:compose-material3 extended name $i",
                description = null,
                website = null,
                developers = emptyList(),
                organization = null,
                scm = null,
                licenses = setOf(License(name = "Apache License 2.0", url = null, year = null, spdxId = "Apache-2.0", licenseContent = null, hash = "a$i")),
            )
        }
        LicencesList(libs, open = null, onToggle = {})
    }

    private companion object {
        val HAS_TEXT = SemanticsMatcher.keyIsDefined(SemanticsProperties.Text)
        val CLICKABLE = SemanticsMatcher.keyIsDefined(SemanticsActions.OnClick)
        val TIME = Regex("""\d{1,2}:\d{2}(\s?[AaPp][Mm])?""")
        const val SWIPES = 30
        const val MORPH = 0.35f
        const val FULL = 0.9f
        const val TOLERANCE = 1.5f
        val OPTIONS = RoborazziOptions(compareOptions = RoborazziOptions.CompareOptions(changeThreshold = 0.02f))
    }
}

private const val SDK = 35

/** Pixel Watch 3 45 mm / Galaxy Watch 7 44 mm class: WearDevices.LARGE_ROUND. */
@Config(sdk = [SDK], qualifiers = "en-w227dp-h227dp-round-watch-xhdpi", application = Application::class)
class LargeRoundTest : WatchShapeTest() {
    override val device = "large_round"
}

/** WearDevices.SMALL_ROUND: 192 dp. */
@Config(sdk = [SDK], qualifiers = "en-w192dp-h192dp-round-watch-xhdpi", application = Application::class)
class SmallRoundTest : WatchShapeTest() {
    override val device = "small_round"
}

/** WearDevices.SQUARE. */
@Config(sdk = [SDK], qualifiers = "en-w180dp-h180dp-notround-watch-xhdpi", application = Application::class)
class SquareTest : WatchShapeTest() {
    override val device = "square"
    override val round = false
}

/** The smallest round watch at the largest font the system offers. */
@Config(sdk = [SDK], qualifiers = "en-w192dp-h192dp-round-watch-xhdpi", fontScale = 1.24f, application = Application::class)
class SmallRoundLargestFontTest : WatchShapeTest() {
    override val device = "small_round_font_largest"
}

/** Romanian runs longer than English; the smallest watch is the hard case. */
@Config(sdk = [SDK], qualifiers = "ro-w192dp-h192dp-round-watch-xhdpi", application = Application::class)
class SmallRoundRomanianTest : WatchShapeTest() {
    override val device = "small_round_ro"
}
