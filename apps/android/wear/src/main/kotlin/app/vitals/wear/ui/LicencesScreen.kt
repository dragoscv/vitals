package app.vitals.wear.ui

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.wear.compose.foundation.lazy.TransformingLazyColumn
import androidx.wear.compose.foundation.lazy.items
import androidx.wear.compose.foundation.lazy.rememberTransformingLazyColumnState
import androidx.wear.compose.material3.ListHeader
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.SurfaceTransformation
import androidx.wear.compose.material3.Text
import androidx.wear.compose.material3.TitleCard
import androidx.wear.compose.material3.lazy.rememberTransformationSpec
import androidx.wear.compose.material3.lazy.transformedHeight
import app.vitals.wear.R
import com.mikepenz.aboutlibraries.Libs
import com.mikepenz.aboutlibraries.entity.Library
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The open-source licences of everything in the watch APK (LEG2-11), from the
 * list the AboutLibraries Gradle plugin writes into res/raw at build time.
 * The watch is a separate binary from the phone, and Apache-2.0 and MIT want
 * the notice to travel with each binary; the phone's screen does not cover
 * this one. Tapping a library shows its licence text.
 */
@Composable
fun LicencesScreen() {
    val resources = LocalResources.current
    val listState = rememberTransformingLazyColumnState()
    val spec = rememberTransformationSpec()
    // A few hundred KB of JSON: parsed off the main thread, once per visit.
    val libraries by produceState<List<Library>?>(null) {
        value = withContext(Dispatchers.IO) {
            runCatching {
                val json = resources.openRawResource(R.raw.aboutlibraries).bufferedReader().use { it.readText() }
                Libs.Builder().withJson(json).build().libraries
            }.getOrDefault(emptyList())
        }
    }
    var open by rememberSaveable { mutableStateOf<String?>(null) }

    ScreenScaffold(scrollState = listState) { padding ->
        TransformingLazyColumn(state = listState, contentPadding = padding) {
            item {
                ListHeader(modifier = Modifier.transformedHeight(this, spec), transformation = SurfaceTransformation(spec)) {
                    Text(stringResource(R.string.licences_title))
                }
            }
            item {
                Text(
                    stringResource(R.string.licences_body),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.fillMaxWidth().transformedHeight(this, spec),
                )
            }
            val list = libraries
            if (list == null) {
                item { Message(stringResource(R.string.loading), "", Modifier.transformedHeight(this, spec)) }
                return@TransformingLazyColumn
            }
            items(list, key = { it.uniqueId }) { lib ->
                val expanded = open == lib.uniqueId
                TitleCard(
                    onClick = { open = if (expanded) null else lib.uniqueId },
                    title = { Text(lib.name, maxLines = 2) },
                    subtitle = {
                        Text(
                            listOfNotNull(lib.artifactVersion, lib.licenses.joinToString { it.spdxId ?: it.name })
                                .filter { it.isNotBlank() }
                                .joinToString(" · "),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    },
                    modifier = Modifier.fillMaxWidth().transformedHeight(this, spec),
                    transformation = SurfaceTransformation(spec),
                ) {
                    if (expanded) {
                        lib.licenses.forEach { licence ->
                            Text(licence.licenseContent ?: licence.url.orEmpty(), style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
            }
        }
    }
}
