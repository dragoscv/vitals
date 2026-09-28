package app.vitals.wear.complication

import app.vitals.ui.Format
import app.vitals.wear.R
import app.vitals.wear.data.Headline

class TemperatureComplicationService : MetricComplicationService() {
    override fun value(headline: Headline): Float? = headline.cpuTemp

    /** Above the 95 °C danger band, so a hot CPU fills most of the ring without pinning it. */
    override val max = 110f
    override fun shortText(value: Float): String = Format.celsiusCompact(value)
    override val title = R.string.temp_short
    override val preview = 81f
}
