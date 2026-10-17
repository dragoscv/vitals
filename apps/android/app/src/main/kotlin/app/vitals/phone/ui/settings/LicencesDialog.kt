package app.vitals.phone.ui.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import app.vitals.phone.R
import com.mikepenz.aboutlibraries.Libs
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The open-source licences of everything in this APK, from the list the
 * AboutLibraries Gradle plugin writes into res/raw at build time. Apache-2.0
 * and MIT both require the notice to travel with the binary; a link to a
 * website is not that. Tapping a library shows its licence text.
 */
@Composable
fun LicencesDialog(onDismiss: () -> Unit) {
    val context = LocalContext.current
    // Parsing a few hundred KB of JSON is not for the main thread.
    val libs by produceState<Libs?>(null) {
        value = withContext(Dispatchers.IO) {
            runCatching {
                val json = context.resources.openRawResource(R.raw.aboutlibraries).bufferedReader().use { it.readText() }
                Libs.Builder().withJson(json).build()
            }.getOrNull()
        }
    }
    var open by remember { mutableStateOf<String?>(null) }

    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surface) {
            Column(Modifier.fillMaxSize().padding(top = 24.dp)) {
                Text(
                    stringResource(R.string.settings_licences),
                    style = MaterialTheme.typography.titleLarge,
                    modifier = Modifier.padding(horizontal = 16.dp),
                )
                Text(
                    stringResource(R.string.settings_licences_body),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                )
                LazyColumn(
                    Modifier.weight(1f),
                    contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    items(libs?.libraries.orEmpty(), key = { it.uniqueId }) { lib ->
                        val expanded = open == lib.uniqueId
                        Column(
                            Modifier
                                .fillMaxWidth()
                                .clickable(role = Role.Button) { open = if (expanded) null else lib.uniqueId },
                        ) {
                            Text(lib.name, style = MaterialTheme.typography.bodyLarge)
                            Text(
                                listOfNotNull(lib.artifactVersion, lib.licenses.joinToString { it.spdxId ?: it.name })
                                    .filter { it.isNotBlank() }
                                    .joinToString(" · "),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                            if (expanded) {
                                lib.licenses.forEach { licence ->
                                    Text(
                                        licence.licenseContent ?: licence.url.orEmpty(),
                                        style = MaterialTheme.typography.bodySmall,
                                        modifier = Modifier.padding(top = 8.dp),
                                    )
                                }
                            }
                        }
                    }
                }
                TextButton(onClick = onDismiss, modifier = Modifier.padding(8.dp)) {
                    Text(stringResource(R.string.back))
                }
            }
        }
    }
}
