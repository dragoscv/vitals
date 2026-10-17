package app.vitals.tv.ui.settings

import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.produceState
import androidx.compose.ui.platform.LocalContext
import androidx.tv.material3.ListItem
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import app.vitals.tv.R
import com.mikepenz.aboutlibraries.Libs
import com.mikepenz.aboutlibraries.entity.Library
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The libraries in this APK and their licence texts, from the list the
 * AboutLibraries Gradle plugin writes into res/raw at build time (Apache-2.0
 * and MIT require the notice to travel with the binary). Parsed off the main
 * thread, and only once the panel is opened.
 */
@Composable
fun rememberLibraries(open: Boolean): State<List<Library>> {
    val context = LocalContext.current
    return produceState(emptyList(), open) {
        if (!open) return@produceState
        value = withContext(Dispatchers.IO) {
            runCatching {
                val json = context.resources.openRawResource(R.raw.aboutlibraries).bufferedReader().use { it.readText() }
                Libs.Builder().withJson(json).build().libraries
            }.getOrDefault(emptyList())
        }
    }
}

/** One library; selecting it shows its licence text underneath. */
@Composable
fun LicenceRow(lib: Library, expanded: Boolean, onClick: () -> Unit) {
    ListItem(
        selected = expanded,
        onClick = onClick,
        headlineContent = { Text(lib.name) },
        supportingContent = {
            Column {
                Text(
                    listOfNotNull(lib.artifactVersion, lib.licenses.joinToString { it.spdxId ?: it.name })
                        .filter { it.isNotBlank() }
                        .joinToString(" · "),
                )
                if (expanded) {
                    lib.licenses.forEach { licence ->
                        Text(licence.licenseContent ?: licence.url.orEmpty(), style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
        },
    )
}
