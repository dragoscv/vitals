package app.vitals.wear.complication

import app.vitals.ui.Format
import app.vitals.wear.R
import app.vitals.wear.data.Headline

class CpuComplicationService : MetricComplicationService() {
    override fun value(headline: Headline): Float? = headline.cpu
    override val max = 100f
    override fun shortText(value: Float): String = Format.percentCompact(value)
    override val title = R.string.cpu_short
    override val preview = 34f
}
