package app.vitals.wear.complication

import android.app.PendingIntent
import android.content.Intent
import android.graphics.drawable.Icon
import androidx.wear.watchface.complications.data.ComplicationData
import androidx.wear.watchface.complications.data.ComplicationType
import androidx.wear.watchface.complications.data.MonochromaticImage
import androidx.wear.watchface.complications.data.NoDataComplicationData
import androidx.wear.watchface.complications.data.PlainComplicationText
import androidx.wear.watchface.complications.data.RangedValueComplicationData
import androidx.wear.watchface.complications.data.ShortTextComplicationData
import androidx.wear.watchface.complications.datasource.ComplicationRequest
import androidx.wear.watchface.complications.datasource.SuspendingComplicationDataSourceService
import app.vitals.wear.MainActivity
import app.vitals.wear.R
import app.vitals.wear.VitalsWearApp
import app.vitals.wear.data.Headline

/**
 * One number on a watch face. Subclasses pick the number; this class owns
 * the shapes, so the CPU and temperature complications cannot drift apart.
 */
abstract class MetricComplicationService : SuspendingComplicationDataSourceService() {
    /** The reading, or `null` when the PC did not measure it. */
    protected abstract fun value(headline: Headline): Float?

    protected abstract val max: Float
    protected abstract fun shortText(value: Float): String
    protected abstract val title: Int
    protected abstract val preview: Float

    override suspend fun onComplicationRequest(request: ComplicationRequest): ComplicationData? {
        val repository = VitalsWearApp.from(this).repository
        repository.load()
        val headline = Headline.primary(repository.states.value.values)?.let(Headline::of)
        repository.refreshInBackground()
        val value = headline?.let(::value)
        // An unmeasured reading is "no data", which the face draws as empty —
        // never a ring at zero, which it would draw as "idle".
        return value?.let { build(request.complicationType, it) } ?: NoDataComplicationData()
    }

    override fun getPreviewData(type: ComplicationType): ComplicationData? = build(type, preview)

    private fun build(type: ComplicationType, value: Float): ComplicationData? {
        val label = PlainComplicationText.Builder(getString(title)).build()
        val text = PlainComplicationText.Builder(shortText(value)).build()
        val image = MonochromaticImage.Builder(Icon.createWithResource(this, R.drawable.ic_pulse)).build()
        return when (type) {
            ComplicationType.RANGED_VALUE ->
                RangedValueComplicationData.Builder(value.coerceIn(0f, max), 0f, max, text)
                    .setText(text)
                    .setTitle(label)
                    .setMonochromaticImage(image)
                    .setTapAction(openApp())
                    .build()
            ComplicationType.SHORT_TEXT ->
                ShortTextComplicationData.Builder(text, text)
                    .setTitle(label)
                    .setMonochromaticImage(image)
                    .setTapAction(openApp())
                    .build()
            else -> null
        }
    }

    private fun openApp(): PendingIntent = PendingIntent.getActivity(
        this,
        0,
        Intent(this, MainActivity::class.java),
        PendingIntent.FLAG_IMMUTABLE,
    )
}
